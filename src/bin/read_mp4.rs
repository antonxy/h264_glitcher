use std::{convert::TryInto, fs::File, io::{Read, Seek}, path::{Path, PathBuf}};
use bitstream_io::{BigEndian, BitReader};
use ffmpeg_next as ffmpeg;
use h264_glitcher::h264::{NalUnit, Sps};
use structopt::StructOpt;

struct AvccConfig {
    nal_length_size: usize,
    sps_list: Vec<Vec<u8>>,
    pps_list: Vec<Vec<u8>>,
}

fn parse_avcc_extradata(extradata: &[u8]) -> Result<AvccConfig, &'static str> {
    if extradata.len() < 7 {
        return Err("Extradata too short for AVCDecoderConfigurationRecord");
    }

    // Byte 4 lower 2 bits + 1 gives the NAL length prefix size (usually 4 bytes)
    let nal_length_size = ((extradata[4] & 0x03) + 1) as usize;
    let mut offset = 5;

    // Extract SPS
    let num_sps = (extradata[offset] & 0x1F) as usize;
    offset += 1;
    let mut sps_list = Vec::with_capacity(num_sps);

    for _ in 0..num_sps {
        if offset + 2 > extradata.len() {
            return Err("Invalid SPS length");
        }
        let sps_len = u16::from_be_bytes([extradata[offset], extradata[offset + 1]]) as usize;
        offset += 2;

        if offset + sps_len > extradata.len() {
            return Err("Truncated SPS data");
        }
        sps_list.push(extradata[offset..offset + sps_len].to_vec());
        offset += sps_len;
    }

    // Extract PPS
    if offset >= extradata.len() {
        return Err("Missing PPS count");
    }
    let num_pps = extradata[offset] as usize;
    offset += 1;
    let mut pps_list = Vec::with_capacity(num_pps);

    for _ in 0..num_pps {
        if offset + 2 > extradata.len() {
            return Err("Invalid PPS length");
        }
        let pps_len = u16::from_be_bytes([extradata[offset], extradata[offset + 1]]) as usize;
        offset += 2;

        if offset + pps_len > extradata.len() {
            return Err("Truncated PPS data");
        }
        pps_list.push(extradata[offset..offset + pps_len].to_vec());
        offset += pps_len;
    }

    Ok(AvccConfig {
        nal_length_size,
        sps_list,
        pps_list,
    })
}

fn extract_packet_nals<'a>(data: &'a [u8], nal_length_size: usize) -> Vec<(usize, &'a [u8])> {
    let mut nals = Vec::new();
    let mut offset = 0;

    while offset + nal_length_size <= data.len() {
        // Read big-endian length prefix (usually 4 bytes, occasionally 1 or 2)
        let nal_len = match nal_length_size {
            4 => u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize,
            2 => u16::from_be_bytes(data[offset..offset + 2].try_into().unwrap()) as usize,
            1 => data[offset] as usize,
            _ => break,
        };
        offset += nal_length_size;

        if offset + nal_len > data.len() {
            break; // Malformed or truncated packet
        }

        // Pure NAL unit slice (starts directly with the 1-byte NAL header)
        nals.push((offset, &data[offset..offset + nal_len]));
        offset += nal_len;
    }

    nals
}

fn process_mp4(input_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    ffmpeg::init()?;
    let mut ictx = ffmpeg::format::input(input_path)?;

    let stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or("No video stream")?;
    let video_index = stream.index();

    // Access extradata via unsafe pointer on parameters (standard in ffmpeg-next)
    // Maybe I don't even really need to parse this
    // Could keep stream.parameters() and pass it to ffmpeg decoder create
    let extradata = unsafe {
        let ptr = (*stream.parameters().as_ptr()).extradata;
        let size = (*stream.parameters().as_ptr()).extradata_size as usize;
        if ptr.is_null() || size == 0 {
            return Err("No extradata found".into());
        }
        std::slice::from_raw_parts(ptr, size)
    };

    // TODO maybe instead of ffmpeg, I could use the more direct mp4parse crate
    // at least it can get the AVCC block
    // not sure if it can get the mp4 packets

    let config = parse_avcc_extradata(extradata)?;
    println!("Extracted {} SPS and {} PPS", config.sps_list.len(), config.pps_list.len());
    let sps_nal = NalUnit::from_bytes(config.sps_list[0].as_slice()).unwrap();
    let sps = Sps::read(&mut BitReader::endian(sps_nal.rbsp.as_slice(), BigEndian)).unwrap();
    println!("SPS {:?}", sps);

    for (stream, packet) in ictx.packets() {
        if stream.index() != video_index {
            continue;
        }
        let position = packet.position(); // Byte position in stream
        println!("Packet at {}", position);
        if let Some(data) = packet.data() {
            let nals = extract_packet_nals(data, config.nal_length_size);
            for (offset, nal) in nals {
                // let nal_type = nal[0] & 0x1F;
                // nal is the raw NAL unit without start codes or length prefixes
                // println!("NAL type: {}, size: {} bytes", nal_type, nal.len());
                let parsed_nal = NalUnit::from_bytes(&nal).unwrap();
                println!("NAL type: {:?}", parsed_nal.nal_unit_type);

                let nal_start = position as usize + offset;
                let nal_length = nal.len();

                let mut buf = vec![0u8; nal.len()];
                let mut file = File::open(input_path).unwrap();
                file.seek(std::io::SeekFrom::Start(position as u64 + offset as u64)).unwrap();
                file.read_exact(&mut buf).unwrap();

                let parsed_nal = NalUnit::from_bytes(&buf).unwrap();
                println!("From file: NAL type: {:?}", parsed_nal.nal_unit_type);
            }
        }
    }

    Ok(())
}

#[derive(Debug, StructOpt)]
#[structopt(name = "read_mp4", about = "Parse and display h264 data from mp4 file")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), required=true, help="Input video file")]
    input: PathBuf,

    #[structopt(short, long, help="Maximum number of NALs")]
    limit: Option<usize>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let opt = Opt::from_args();

    eprintln!("Open file {:?}", opt.input);
    process_mp4(&opt.input)
}
