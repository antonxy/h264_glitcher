extern crate structopt;
use bitstream_io::{BigEndian, BitReader};
use colored::Colorize;
use h264_glitcher::h264::{NALUnitType, NalIterator, NalUnit, Pps, Sps};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use structopt::StructOpt;

mod diff_printing;

#[derive(Debug, StructOpt)]
#[structopt(name = "validate_video_parameters")]
/// Checks for possible encoding issues in videos
///
/// Compares the sequence parameter set (SPS) of the given videos with a given reference videos.
/// Ideally the SPSs are identical. Differences might indicate incompatible encoding options.
/// However not every difference shown is actually problematic.
///
/// Also checks if some assumptions we make in the software are violated by any of the videos.
struct Opt {
    #[structopt(short, long, parse(from_os_str), required = true)]
    /// Reference video file
    ///
    /// This should be a known working video file
    reference: PathBuf,

    #[structopt(short, long, parse(from_os_str), required = true)]
    /// Video files to check
    input: Vec<PathBuf>,

    #[structopt(short, long)]
    /// Whether to show the specific differences found
    diff: bool,
}

fn check_for_assumptions(sps: &Sps) {
    if sps.separate_colour_plane_flag {
        println!(
            "{}",
            "separate_colour_plane_flag is set. We are assuming it is not set.".red()
        );
    }
    if sps.log2_max_frame_num_minus4 != 0 {
        println!(
            "{}",
            "log2_max_frame_num_minus4 is != 0. We are assuming it is 0.".red()
        );
    }
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();

    let mut reference_sps = None;
    let mut reference_pps = None;

    let open_video_file = |path| -> Result<_, std::io::Error> {
        let input_file = File::open(path)?;
        let file = std::io::BufReader::with_capacity(1 << 20, input_file);

        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
        Ok(it.map(|v| NalUnit::from_bytes(&v)))
    };

    for nal_unit in open_video_file(opt.reference.clone())? {
        match nal_unit {
            Err(e) => println!("Failed to parse NAL: {:?}", e),
            Ok(nal_unit) => match nal_unit.nal_unit_type {
                NALUnitType::Sps => {
                    let sps =
                        Sps::read(&mut BitReader::endian(nal_unit.rbsp.as_slice(), BigEndian));
                    match sps {
                        Err(e) => println!("Failed to parse reference SPS: {:?}", e),
                        Ok(sps) => {
                            println!(
                                "Reference video file {:?}: {}",
                                opt.reference,
                                "Found reference SPS".green()
                            );
                            if opt.diff {
                                println!("{:#?}", sps);
                            }
                            check_for_assumptions(&sps);
                            reference_sps = Some(sps);
                        }
                    }
                }
                NALUnitType::Pps => {
                    let pps =
                        Pps::read(&mut BitReader::endian(nal_unit.rbsp.as_slice(), BigEndian));
                    match pps {
                        Err(e) => println!("Failed to parse reference PPS: {:?}", e),
                        Ok(pps) => {
                            println!(
                                "Reference video file {:?}: {}",
                                opt.reference,
                                "Found reference PPS".green()
                            );
                            if opt.diff {
                                println!("{:#?}", pps);
                            }
                            reference_pps = Some(pps);
                        }
                    }
                }
                _ => {}
            },
        }
    }
    println!("------");

    if opt.diff {
        println!("Showing diff as {}, {}", "reference".green(), "value".red());
    }

    let reference_sps = reference_sps.unwrap();
    let reference_pps = reference_pps.unwrap();

    // Ignore directories
    let paths: Vec<PathBuf> = opt.input.into_iter().filter(|p| p.is_file()).collect();

    for path in paths {
        for nal_unit in open_video_file(path.clone())? {
            match nal_unit {
                Err(e) => println!("Failed to parse NAL: {:?}", e),
                Ok(nal_unit) => match nal_unit.nal_unit_type {
                    NALUnitType::Sps => {
                        let sps =
                            Sps::read(&mut BitReader::endian(nal_unit.rbsp.as_slice(), BigEndian));
                        match sps {
                            Err(e) => println!("Failed to parse SPS: {:?}", e),
                            Ok(sps) => {
                                if sps != reference_sps {
                                    println!(
                                        "Video file {:?}: {}",
                                        path,
                                        "SPS differs from reference".red()
                                    );
                                    if opt.diff {
                                        diff_printing::print_diff(&reference_sps, &sps);
                                    }
                                } else {
                                    println!("Video file {:?}: {}", path, "Same SPS".green());
                                }
                                check_for_assumptions(&sps);
                            }
                        }
                    }
                    NALUnitType::Pps => {
                        let pps =
                            Pps::read(&mut BitReader::endian(nal_unit.rbsp.as_slice(), BigEndian));
                        match pps {
                            Err(e) => println!("Failed to parse PPS: {:?}", e),
                            Ok(pps) => {
                                if pps != reference_pps {
                                    println!(
                                        "Video file {:?}: {}",
                                        path,
                                        "PPS differs from reference".red()
                                    );
                                    if opt.diff {
                                        diff_printing::print_diff(&reference_pps, &pps);
                                    }
                                } else {
                                    println!("Video file {:?}: {}", path, "Same PPS".green());
                                }
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    Ok(())
}
