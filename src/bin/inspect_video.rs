extern crate structopt;
use std::fs::File;
use structopt::StructOpt;
use std::path::PathBuf;
use std::io::Read;
use bitstream_io::{BitReader, BigEndian};
use h264_glitcher::h264::{NalIterator, NalUnit, NALUnitType, SliceHeader, Sps, Pps};


#[derive(Debug, StructOpt)]
#[structopt(name = "inspect_video", about = "Parse and display the syntax of a h264 stream")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), required=true, help="Input video file")]
    input: PathBuf,

    #[structopt(short, long, help="Maximum number of NALs")]
    limit: Option<usize>,
}


fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();

    eprintln!("Open file {:?}", opt.input);
    let input_file = File::open(opt.input)?;
    let file = std::io::BufReader::with_capacity(1<<20, input_file);

    let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
    let it = it.map(|v| NalUnit::from_bytes(&v));

    let it = it.take(opt.limit.unwrap_or(usize::MAX));

    let mut current_sps = None;
    let mut current_pps = None;

    for nal_unit in it {
        match nal_unit {
            Err(e) => println!("Failed to parse NAL: {:?}", e),
            Ok(nal_unit) => {
                println!("{}", nal_unit);
                match nal_unit.nal_unit_type {
                    NALUnitType::Sps => {
                        let sps = Sps::read(&mut BitReader::endian(nal_unit.rbsp.as_slice(), BigEndian));
                        println!("{:?}", nal_unit.rbsp);
                        match sps {
                            Err(e) => println!("Failed to parse SPS: {:?}", e),
                            Ok(sps) => {
                                println!("{:?}", sps);
                                current_sps = Some(sps);
                            }
                        }
                    }
                    NALUnitType::Pps => {
                        let pps = Pps::read(&mut BitReader::endian(nal_unit.rbsp.as_slice(), BigEndian));
                        println!("{:?}", nal_unit.rbsp);
                        match pps {
                            Err(e) => println!("Failed to parse SPS: {:?}", e),
                            Ok(pps) => {
                                println!("{:?}", pps);
                                current_pps = Some(pps);
                            }
                        }
                    }
                    NALUnitType::CodedSliceIdr | NALUnitType::CodedSliceNonIdr => {
                        let header = SliceHeader::from_bytes(&nal_unit.rbsp, current_sps.as_ref().unwrap(), current_pps.as_ref().unwrap(), &nal_unit.nal_unit_type, nal_unit.nal_ref_idc);
                        match header {
                            Err(e) => println!("Failed to parse slice header: {:?}", e),
                            Ok(header) => println!("{}", header)
                        }
                    },
                    _ => {},
                }
            }

        }
        println!("------");
    }


    Ok(())
}
