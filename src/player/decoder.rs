use ffmpeg_next as ffmpeg;
use ffmpeg_next::codec::decoder::Video as VideoDecoder;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::Video as VideoFrame;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub y_plane: Vec<u8>,
    pub u_plane: Vec<u8>,
    pub v_plane: Vec<u8>,
}

/// Start decoder thread reading from a given ffmpeg input path
pub fn start_decoder_thread_from_path(input_path: String) -> Receiver<DecodedFrame> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        if let Err(e) = decoder_loop(tx, &input_path) {
            eprintln!("Decoder error: {}", e);
        }
    });

    rx
}

fn decoder_loop(tx: Sender<DecodedFrame>, input_path: &str) -> Result<(), ffmpeg::Error> {
    ffmpeg::init()?;

    // Use ffmpeg's demuxer to read raw H264 from the given input
    let mut ictx = ffmpeg::format::input(&input_path)?;

    let video_stream_index = ictx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or(ffmpeg::Error::StreamNotFound)?
        .index();

    let stream = ictx.stream(video_stream_index).ok_or(ffmpeg::Error::StreamNotFound)?;
    let mut decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?
        .decoder()
        .video()?;

    loop {
        let mut packet = ffmpeg::packet::Packet::empty();
        match packet.read(&mut ictx) {
            Ok(_) => {
                if packet.stream() == video_stream_index {
                    let _ = decoder.send_packet(&packet);
                    let _ = receive_frames(&mut decoder, &tx);
                }
            }
            Err(ffmpeg::Error::Eof) => break,
            Err(_) => continue,
        }
    }

    let _ = decoder.send_eof();
    let _ = receive_frames(&mut decoder, &tx);

    Ok(())
}

fn receive_frames(
    decoder: &mut VideoDecoder,
    tx: &Sender<DecodedFrame>,
) -> Result<(), ffmpeg::Error> {
    let mut decoded = VideoFrame::empty();

    while decoder.receive_frame(&mut decoded).is_ok() {
        if decoded.format() != Pixel::YUV420P {
            eprintln!("Warning: frame format is {:?}, expected YUV420P", decoded.format());
        }

        let width = decoded.width();
        let height = decoded.height();

        let y_plane = extract_plane(&decoded, 0, width as usize, height as usize);
        let u_plane = extract_plane(&decoded, 1, (width / 2) as usize, (height / 2) as usize);
        let v_plane = extract_plane(&decoded, 2, (width / 2) as usize, (height / 2) as usize);

        let frame = DecodedFrame {
            width,
            height,
            y_plane,
            u_plane,
            v_plane,
        };

        if tx.send(frame).is_err() {
            break;
        }
    }

    Ok(())
}

fn extract_plane(frame: &VideoFrame, plane_idx: usize, width: usize, height: usize) -> Vec<u8> {
    let data = frame.data(plane_idx);
    let stride = frame.stride(plane_idx);
    let mut plane = Vec::with_capacity(width * height);

    for y in 0..height {
        let row_start = y * stride;
        plane.extend_from_slice(&data[row_start..row_start + width]);
    }

    plane
}
