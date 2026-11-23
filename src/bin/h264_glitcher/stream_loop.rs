use std::ffi::OsStr;
use std::fs::File;
use std::thread;
use std::sync::{Arc, Mutex};
use std::path::{Path, PathBuf};
use crate::{Opt, NalUnit, NALUnitType, SliceHeader, StreamingParams, SigmaDelta};
use h264_glitcher::h264::{NalLocation, NalOffsetIterator};
use std::io::{Read, Seek, SeekFrom, Write};
use std::time::{Instant, Duration};
use h264_glitcher::mp4::index_mp4;

/// Contains an open video file and an index for fast access to NALs
struct LoadedVideo {
    file: File,
    nal_locations: Vec<NalLocation>,
}

impl LoadedVideo {
    fn load(path: &Path) -> std::io::Result<LoadedVideo> {

        if path.extension() == Some(OsStr::new("mp4")) {
            let mp4 = index_mp4(path).or(Err(std::io::Error::new(std::io::ErrorKind::Other, "Indexing mp4 failed")))?;
            let file = File::open(path)?;
            Ok(LoadedVideo { file, nal_locations: mp4.nal_locations })
        }  else {
            let mut file = File::open(path)?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            let nal_locations: Vec<NalLocation> = NalOffsetIterator::new(data.iter().copied()).collect();
            eprintln!("Indexed {} NALs from {:?}", nal_locations.len(), path);

            Ok(LoadedVideo { file, nal_locations })
        }
    }

    fn len(&self) -> usize {
        self.nal_locations.len()
    }

    fn read_nal(&mut self, index: usize) -> Result<NalUnit, String> {
        let loc = *self.nal_locations.get(index)
            .ok_or_else(|| format!("NAL index {} out of bounds", index))?;

        let mut bytes = vec![0u8; loc.length];
        self.file.seek(SeekFrom::Start(loc.offset as u64))
            .map_err(|e| format!("Seek failed: {}", e))?;
        self.file.read_exact(&mut bytes)
            .map_err(|e| format!("Read failed: {}", e))?;

        NalUnit::from_bytes(&bytes).map_err(|e| format!("Parse failed: {:?}", e))
    }
}

struct VideoCache {
    paths: Vec<PathBuf>,
    cache: Vec<Option<LoadedVideo>>,
}

impl VideoCache {
    fn new(paths: Vec<PathBuf>) -> Self {
        let cache: Vec<Option<LoadedVideo>> = (0..paths.len()).map(|_| None).collect();
        VideoCache { paths, cache }
    }

    fn len(&self) -> usize {
        self.paths.len()
    }

    fn get(&mut self, index: usize) -> &mut LoadedVideo {
        if self.cache[index].is_none() {
            let video = LoadedVideo::load(&self.paths[index])
                .expect("Failed to load video");
            self.cache[index] = Some(video);
        }
        self.cache[index].as_mut().unwrap()
    }

    fn preload_all(&mut self) {
        for i in 0..self.paths.len() {
            if self.cache[i].is_none() {
                self.cache[i] = Some(
                    LoadedVideo::load(&self.paths[i])
                    .expect("Failed to load video in preload")
                )
            }
        }
    }
}


pub fn start_stream_thread<W: Write + Send + 'static>(
    paths: Vec<PathBuf>,
    opt: Opt,
    streaming_params: &Arc<Mutex<StreamingParams>>,
    writer: W,
) {
    let streaming_params = streaming_params.clone();
    thread::spawn(move || {
        stream_loop(paths, opt, streaming_params, writer);
    });
}

fn stream_loop<W: Write>(paths : Vec<PathBuf>, opt: Opt, streaming_params: Arc<Mutex<StreamingParams>>, mut writer: W) {
    let mut library = VideoCache::new(paths);
    if opt.prefetch {
        library.preload_all();
    }

    writer.write_all(&[0x00, 0x00, 0x00, 0x01]).expect("Writing to stdout failed");

    let mut last_frame_num = 0;
    let rewrite_frame_nums = true;
    let mut write_frame = move |nal_unit: &NalUnit| -> std::io::Result<()> {
        let mut nal_unit = nal_unit.clone();
        let has_frame_num = matches!(nal_unit.nal_unit_type, NALUnitType::CodedSliceIdr | NALUnitType::CodedSliceNonIdr);
        if rewrite_frame_nums && has_frame_num {
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
            nal_unit.rbsp = header.to_bytes();

        }
        writer.write_all(&nal_unit.to_bytes())?;
        writer.write_all(&[0x00, 0x00, 0x00, 0x01])?;
        writer.flush()?;
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
            write_frame(&nal_unit).expect("Could not write first I-frame");
            if nal_unit.nal_unit_type == NALUnitType::CodedSliceIdr {
                eprintln!("Got first I frame");
                break;
            }
        }
    }

    let mut sd = SigmaDelta::new();
    let mut frame_repeats_left: i32 = 0;


// Pseudocode
/*

Process requests from state

Calculate next frame time
Wait for next frame, max 100ms
if wait timed out, recalculate next frame time (in case FPS changed)

Process beat if one happened in the meantime

*/

    let mut last_frame_time = Instant::now();
    let mut last_beat_time = Instant::now();

    loop {
        let mut params : StreamingParams = streaming_params.lock().unwrap().clone();
        let state = params.active_state().clone();

        // Switch video if requested
        if current_video_num as i32 != *state.video_num && *state.video_num < library.len() as i32 {
            current_video_num = *state.video_num as usize;
            current_frame = 0;
        }

        if params.restart_loop {
            current_frame = 0;
            streaming_params.lock().unwrap().restart_loop = false;
        }

        let fps = *state.fps;
        let next_frame_time = last_frame_time + Duration::from_secs_f32(1.0/fps);
        let sleep_duration = (next_frame_time - Instant::now()).min(Duration::from_millis(100)); // Sleep at most 100ms so that state changes can be handled quickly enough
        std::thread::sleep(sleep_duration);
        let current_frame_time = Instant::now();
        if current_frame_time < next_frame_time {
            // If we woke up early, restart the loop so that we update the FPS and handle request, in case changes came in inbetween
            continue;
        }

        // TODO Here we can handle beat based stuff
        // If the FPS is low it could be a bit delayed vs the beat. But that might be fine

        // Emit the next frame

        let video = library.get(current_video_num);

        // Keep going until we found a non-I-frame
        // TODO could hang if we have only I frames in a video for whatever reason
        loop {
            // Restart video if at end
            // TODO only advance frame if we repeated it often enough
            advance_frame(&mut current_frame, video.len());
            let nal_unit = video.read_nal(current_frame).expect("Failed to read NAL");

            // If pass_iframe is not activated, send only CodedSliceNonIdr
            // Sending a new SPS without an Idr Slice seems to cause problems when switching between some videos
            if nal_unit.nal_unit_type == NALUnitType::CodedSliceNonIdr || *state.pass_iframe {
                //TODO use sigma delta encoder to decide whether to drop/repeat the frame
                //How do I repeat a frame nicely in this loop? Do I want to sleep here? Maybe better
                //restart the loop. But how does it interact with the redt
                // Maybe use a counter somewhere and only `advance_frame` if the counter is reached

                write_frame(&nal_unit).expect("failed to write frame to stdout");
                if nal_unit.nal_unit_type.is_picture_data() {
                    last_frame_time = current_frame_time;
                    break;
                }
            }
        }
    }

}
