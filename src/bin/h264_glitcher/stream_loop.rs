use std::thread;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use crate::{Opt, VideoLibrary, NalUnit, NALUnitType, SliceHeader, StreamingParams, SigmaDelta};
use h264_glitcher::fps_loop::LoopTimer;
use std::net::UdpSocket;
use std::io::Write;
use rand::Rng;

pub fn start_stream_thread<W: Write + Send + 'static>(
    paths: Vec<PathBuf>,
    opt: Opt,
    streaming_params: &Arc<Mutex<StreamingParams>>,
    loop_timer: LoopTimer,
    send_sock: &Arc<Mutex<UdpSocket>>,
    writer: W,
) {
    let streaming_params = streaming_params.clone();
    let send_sock = send_sock.clone();
    thread::spawn(move || {
        stream_loop(paths, opt, streaming_params, loop_timer, send_sock, writer);
    });
}

fn stream_loop<W: Write>(paths : Vec<PathBuf>, opt: Opt, streaming_params: Arc<Mutex<StreamingParams>>, mut loop_timer: LoopTimer, send_sock: Arc<Mutex<UdpSocket>>, mut handle: W) {
    let mut library = VideoLibrary::new(paths);
    if opt.prefetch {
        library.preload_all();
    }

    handle.write_all(&[0x00, 0x00, 0x00, 0x01]).expect("Writing to stdout failed");



    let mut rng = rand::thread_rng();
    let mut last_frame_num = 0;
    let rewrite_frame_nums = true;
    let mut write_frame = move |nal_unit: &NalUnit, byte_errors: f32| -> std::io::Result<()> {
        let mut nal_unit = nal_unit.clone();
        let has_frame_num = matches!(nal_unit.nal_unit_type, NALUnitType::CodedSliceIdr | NALUnitType::CodedSliceNonIdr);
        if (rewrite_frame_nums || byte_errors > 0.0) && has_frame_num {
            let mut header = SliceHeader::from_bytes(&nal_unit.rbsp).unwrap();
            // Just setting all frame nums to zero also seems to work.
            // Maybe mpv even crashes a bit less with just zero
            // I haven't observed a crash for a while though, maybe it was something else also
            //header.frame_num = 0;

            if rewrite_frame_nums {
                header.frame_num = last_frame_num;
                last_frame_num += 1;
                last_frame_num %= 16; //This just assumes frame nums are encoded with 4 bits. That doesn't have to be the case though.
            }
            if byte_errors > 0.0 {
                // Introduce random errors. Start some bytes into buffer so that we hopefully only
                // hit data, not the header
                let offset = 50;
                // let offset = 0; // Or maybe thats fun also?

                for b in header.data[offset..].iter_mut() {
                    if rng.gen::<f32>() < byte_errors {
                        *b = rng.gen()
                    }
                }

                // Reasonable probability of errors seems to be around 0.0001
                // Weirder behaviour at higher error probabilities:
                // Why does the motion always go down ?
                // Why is the playhead not smooth anymore?
                // Does mpv hang and slow down the glitcher also (pipe gets full)?
                // Probably have to parse further down and destroy more controlled regions
                // Would be cool to destroy whole blocks, would look more glitchy maybe
            }
            nal_unit.rbsp = header.to_bytes();

        }
        handle.write_all(&nal_unit.to_bytes())?;
        handle.write_all(&[0x00, 0x00, 0x00, 0x01])?;
        handle.flush()?;
        Ok(())
    };
    let mut current_video_num: usize = 0;
    let mut current_frame: usize = 0;


    let advance_frame = |current_frame: &mut usize, total_frames: usize| {
        let (mut from_incl, mut to_excl) = (0, total_frames);

        //TODO don't lock every fucking time
        let mut params = streaming_params.lock().unwrap();

        if let Some(short_loop) = &mut params.short_loop {
            if short_loop.first_frame.is_none() {
                short_loop.first_frame = Some(*current_frame);
            }

            let loop_from = short_loop.first_frame.unwrap();
            let loop_to = short_loop.first_frame.unwrap() + short_loop.len;

            from_incl = usize::min(loop_from, total_frames - 2);
            to_excl = usize::min(usize::max(from_incl + 1, loop_to), total_frames);
        } else if let Some((loop_from, loop_to)) = params.active_state().loop_range.0 {
            let loop_from = (total_frames as f32 * loop_from) as usize;
            let loop_to = (total_frames as f32 * loop_to) as usize;

            from_incl = usize::min(loop_from, total_frames - 2);
            to_excl = usize::min(usize::max(from_incl + 1, loop_to), total_frames);
        }

        assert!(from_incl >= 0);
        assert!(from_incl < to_excl);
        assert!(to_excl <= total_frames);

        if *current_frame < from_incl {
            *current_frame = from_incl;
        } else {
            *current_frame += 1;
            if *current_frame >= to_excl {
                *current_frame = from_incl;
            }
        }
    };

    // Write out at least one I-frame
    {
        loop {
            let video = library.get(current_video_num);
            let nal_unit = video.read_nal(current_frame).expect("Failed to read NAL");
            advance_frame(&mut current_frame, video.len());
            write_frame(&nal_unit, 0.0).expect("Could not write first I-frame");
            if nal_unit.nal_unit_type == NALUnitType::CodedSliceIdr {
                eprintln!("Got first I frame");
                break;
            }
        }
    }

    let mut sd = SigmaDelta::new();
    let mut fps = 30.0_f32;
    let mut frame_repeats_left: i32 = 0;

    loop {
        loop_timer.begin_loop();
        let mut params = streaming_params.lock().unwrap().clone();
        let state = params.active_state().clone();

        // Process all "requests"

        // Switch video if requested
        if current_video_num as i32 != *state.video_num && *state.video_num < library.len() as i32 {
            current_video_num = *state.video_num as usize;
            current_frame = 0;
        }

        if params.restart_loop {
            current_frame = 0; // Will be set to loop start by advance_frame(...)
            streaming_params.lock().unwrap().restart_loop = false;
        }

        if let Some(skip) = params.skip_frames {
            for _ in 0..skip {
                advance_frame(&mut current_frame, library.get(current_video_num).len()); //TODO advance n
            }
            streaming_params.lock().unwrap().skip_frames = None;
        }

        // Now the state based stuff

        let frame_repeat = sd.put(*state.frame_repeat);

        // Restart video if at end
        let video = library.get(current_video_num);
        advance_frame(&mut current_frame, video.len());
        let nal_unit = video.read_nal(current_frame).expect("Failed to read NAL");

        for _ in 0..frame_repeat {

            // If pass_iframe is not activated, send only CodedSliceNonIdr
            // Sending a new SPS without an Idr Slice seems to cause problems when switching between some videos
            if nal_unit.nal_unit_type == NALUnitType::CodedSliceNonIdr || *state.pass_iframe {
                //TODO use sigma delta encoder to decide whether to drop/repeat the frame
                //How do I repeat a frame nicely in this loop? Do I want to sleep here? Maybe better
                //restart the loop. But how does it interact with the redt
                // Maybe use a counter somewhere and only `advance_frame` if the counter is reached

                write_frame(&nal_unit, *state.byte_errors).expect("failed to write frame to stdout");
                let is_picture_data = nal_unit.nal_unit_type.is_picture_data();
                if !is_picture_data {
                    continue; //Only sleep if the nal_unit is a video frame
                }
            } else {
                continue; //If we didn't send out frame don't sleep
            }

            let playhead = current_frame as f32 / library.get(current_video_num).len() as f32;
            {
                let mut streaming_params = streaming_params.lock().unwrap();
                streaming_params.active_state_mut().playhead.set(playhead);
                if let Some(addr) = params.client_addr {
                    streaming_params.send_changed(&send_sock.lock().unwrap(), &addr);
                }
            }

            loop_timer.end_loop();

        }

    }
}
