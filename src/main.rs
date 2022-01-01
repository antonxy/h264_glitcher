pub(crate) mod h264;
pub(crate) mod h264_iterator;
pub mod libh264bitstream;

extern crate enum_primitive;
extern crate rand;
extern crate structopt;

use enum_primitive::*;
use h264::FrameType;
use crate::libh264bitstream::*;
use std::convert::TryInto;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::io::{Seek, SeekFrom};
use std::os::raw::c_int;
use std::path::PathBuf;
use std::str::FromStr;
use std::vec::Vec;
use structopt::StructOpt;
use std::thread;
use std::sync::{Mutex, Arc};
use std::net::{SocketAddrV4, UdpSocket};
use rosc::OscPacket;

enum_from_primitive! {
    #[derive(Debug, Copy, Clone, PartialEq)]
    #[repr(u8)]
    enum NALUnitType {
        Unpecified = 0,
        CodedSliceNonIdr          = 1,
        CodedSliceDataPartitionA = 2,
        CodedSliceDataPartitionB = 3,
        CodedSliceDataPartitionC = 4,
        CodedSliceIdr              = 5,
        Sei                          = 6,
        Sps                          = 7,
        Pps                          = 8,
        Aud                          = 9,
        EndOfSequence              = 10,
        EndOfStream                = 11,
        Filler                       = 12,
        SpsExt                      = 13,
        PrefixNal                   = 14,
        SubsetSps                   = 15,
        Dps                          = 16,
        CodedSliceAux              = 19,
        CodedSliceSvcExtension    = 20,
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "datamosher", about = "Removes I Frames from h264 stream")]
struct Opt {
    #[structopt(short, long, parse(from_os_str))]
    input: PathBuf,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    CreateEditTable(CreateEditTableCmd),
    ApplyEditTable(ApplyEditTableCmd),
    ApplyRandomEdits(ApplyRandomEditsCmd),
    QuickMode(QuickModeCmd),
    Streaming(StreamingCmd),
}

#[derive(Debug, StructOpt)]
struct CreateEditTableCmd {
    #[structopt(short, long, parse(from_os_str))]
    edit_table_out: PathBuf,

    #[structopt(short, long)]
    frames: Option<usize>,
}

#[derive(Debug, StructOpt)]
struct ApplyEditTableCmd {
    #[structopt(short, long, parse(from_os_str))]
    edit_table_in: PathBuf,
    #[structopt(short, long, parse(from_os_str))]
    output: PathBuf,
}

#[derive(Debug, StructOpt)]
struct ApplyRandomEditsCmd {
    #[structopt(short, long, parse(from_os_str))]
    edit_table_in: PathBuf,

    #[structopt(short = "o", long, parse(from_os_str))]
    edit_table_out: PathBuf,

    #[structopt(short = "p", long, default_value = "0.0")]
    iframe_prob: f32,

    #[structopt(short, long, default_value = "0")]
    reorder_iframes: usize,
}

#[derive(Debug, StructOpt)]
struct QuickModeCmd {
    #[structopt(short = "p", long, default_value = "0.0")]
    iframe_prob: f32,

    #[structopt(short, long, default_value = "0")]
    reorder_iframes: usize,

    #[structopt(short, long, parse(from_os_str))]
    output: PathBuf,
}

#[derive(Debug, StructOpt)]
struct StreamingCmd {
    #[structopt(short = "l", long, default_value = "0.0.0.0:8000")]
    listen_addr: String,
}

#[derive(Debug)]
struct EditTableEntry {
    nal_start: usize,
    nal_end: usize,
    iframe: bool,
}

impl EditTableEntry {
    fn to_string(&self) -> String {
        format!(
            "0x{:06x}-0x{:06x}-{:5}",
            self.nal_start, self.nal_end, self.iframe
        )
    }
    fn from_string(string: &str) -> EditTableEntry {
        let mut it = string.split("-");
        let start_str = it.next().unwrap();
        let end_str = it.next().unwrap();
        let iframe_str = it.next().unwrap();
        let parse_hex =
            |s: &str| usize::from_str_radix(s.trim().trim_start_matches("0x"), 16).unwrap();
        let parse_bool = |s: &str| bool::from_str(s.trim()).unwrap();
        EditTableEntry {
            nal_start: parse_hex(start_str),
            nal_end: parse_hex(end_str),
            iframe: parse_bool(iframe_str),
        }
    }
}

fn create_edit_table(input_file: &mut File, opt: &CreateEditTableCmd) -> std::io::Result<()> {
    let mut buf = Vec::new();
    input_file.read_to_end(&mut buf)?;

    let mut edit_table_out = File::create(&opt.edit_table_out)?;
    unsafe {
        let h2 = h264_new();
        let mut start_offset: usize = 0;
        let mut i = 0;
        while start_offset < buf.len() && opt.frames.map_or(true, |f| i < f) {
            let mut nal_start: c_int = 0;
            let mut nal_end: c_int = 0;
            find_nal_unit(
                buf.as_mut_ptr().add(start_offset),
                (buf.len() - start_offset).try_into().unwrap(),
                &mut nal_start,
                &mut nal_end,
            );
            let nal_start_glob = nal_start + start_offset as i32;
            let nal_end_glob = nal_end + start_offset as i32;
            let nal_size = nal_end - nal_start;
            let ret = read_nal_unit(
                h2,
                buf.as_mut_ptr().add(nal_start_glob.try_into().unwrap()),
                nal_size,
            );
            assert_eq!(ret, nal_size);

            let frame_type = FrameType::from_sh_slice_type((*(*h2).sh).slice_type);

            let entry = EditTableEntry {
                nal_start: nal_start_glob as usize,
                nal_end: nal_end_glob as usize,
                iframe: frame_type == FrameType::IOnly,
            };

            edit_table_out.write_all(
                format!(
                    "{} | dec {:06} | Type {:?}\n",
                    entry.to_string(),
                    nal_start_glob,
                    frame_type
                )
                .as_bytes(),
            )?;

            //let nal_unit_type = NALUnitType::from_i32((*(*h2).nal).nal_unit_type).unwrap();
            //println!("Frame {:?} - FrameType {:?} - NAL Unit Type {:?} - NAL Size {:}",
            //         (*(*h2).sh).frame_num,
            //         frame_type,
            //         nal_unit_type,
            //         nal_size
            //         );

            start_offset += nal_end as usize;
            i += 1;
        }
        h264_free(h2);
    }
    Ok(())
}

fn apply_edit_table(input_file: &mut File, opt: &ApplyEditTableCmd) -> std::io::Result<()> {
    let mut buf = Vec::new();
    input_file.read_to_end(&mut buf)?;

    let edit_table_file = File::open(&opt.edit_table_in)?;
    let edit_table_reader = BufReader::new(edit_table_file);

    let mut outfile = File::create(&opt.output)?;
    for line in edit_table_reader.lines().map(|l| l.unwrap()) {
        if line.starts_with("#") {
            continue;
        }
        let entry_str = line.split("|").next().unwrap();
        let entry = EditTableEntry::from_string(entry_str);
        outfile.write_all(&buf[(entry.nal_start - 4)..entry.nal_end])?;
    }
    Ok(())
}

struct LineInfo {
    line: String,
    entry: EditTableEntry,
    delete_line: bool,
}

fn apply_random_edits(opt: &ApplyRandomEditsCmd) -> std::io::Result<()> {
    let edit_table_file = File::open(&opt.edit_table_in)?;
    let edit_table_reader = BufReader::new(edit_table_file);

    let mut lines = Vec::new();

    let mut num_iframes = 0;
    for line in edit_table_reader.lines().map(|l| l.unwrap()) {
        //if line.starts_with("#") {
        //    continue;
        //}
        let entry_str = line.split("|").next().unwrap();
        let entry = EditTableEntry::from_string(entry_str);
        let delete_line =
            entry.iframe && rand::random::<f32>() > opt.iframe_prob && num_iframes >= 1;
        if entry.iframe {
            num_iframes += 1;
        }
        lines.push(LineInfo {
            line: line,
            entry: entry,
            delete_line: delete_line,
        });
    }

    let num_iframes = lines
        .iter()
        .enumerate()
        .filter(|(_idx, el)| el.entry.iframe && !el.delete_line)
        .count();
    for _ in 0..opt.reorder_iframes {
        let mut start = lines
            .iter()
            .enumerate()
            .cycle()
            .filter(|(_idx, el)| el.entry.iframe && !el.delete_line);
        let start_el = start.nth(rand::random::<usize>() % num_iframes).unwrap().0;
        let end_el = start.nth(4).unwrap().0;
        println!("Swap {}, {}", start_el, end_el);
        lines.swap(start_el, end_el);
    }

    let mut edit_table_out = File::create(&opt.edit_table_out)?;
    for line in lines {
        if line.delete_line {
            edit_table_out.write_all("#".as_bytes())?;
        }
        edit_table_out.write_all(format!("{}\n", line.line).as_bytes())?;
    }

    Ok(())
}

fn quick_mode(input_file: &mut File, opt: &QuickModeCmd) -> std::io::Result<()> {
    let create_opts = CreateEditTableCmd {
        edit_table_out: PathBuf::from("/tmp/edtab"),
        frames: None,
    };
    create_edit_table(input_file, &create_opts)?;
    let rand_ed = ApplyRandomEditsCmd {
        edit_table_in: PathBuf::from("/tmp/edtab"),
        edit_table_out: PathBuf::from("/tmp/edtabo"),
        iframe_prob: opt.iframe_prob,
        reorder_iframes: opt.reorder_iframes,
    };
    apply_random_edits(&rand_ed)?;
    let apply_cmd = ApplyEditTableCmd {
        edit_table_in: PathBuf::from("/tmp/edtabo"),
        output: opt.output.clone(),
    };
    input_file.seek(SeekFrom::Start(0))?;
    apply_edit_table(input_file, &apply_cmd)?;
    Ok(())
}

#[derive(Default)]
struct StreamingParams {
    fps: f32,
}


fn streaming_mode(opt: &StreamingCmd) -> std::io::Result<()> {
    let streaming_params = Arc::new(Mutex::new(StreamingParams::default()));
    let addr = match SocketAddrV4::from_str(&opt.listen_addr) {
        Ok(addr) => addr,
        Err(_) => panic!("Invalid listen_addr"),
    };
    thread::spawn(move || {
        osc_listener(&addr,streaming_params.clone());
    });
    loop {}
    Ok(())
}

fn osc_listener(addr: &SocketAddrV4, streaming_params: Arc<Mutex<StreamingParams>>) {
    let sock = UdpSocket::bind(addr).unwrap();
    println!("OSC: Listening to {}", addr);

    let mut buf = [0u8; rosc::decoder::MTU];

    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, _)) => {
                let packet = rosc::decoder::decode(&buf[..size]).unwrap();
                match packet {
                    OscPacket::Message(msg) => {
                        match msg.addr.as_str() {
                            "/fps" => {
                                let mut params = streaming_params.lock().unwrap();
                                params.fps = msg.args[0].clone().float().unwrap();
                            },
                            _ => {
                                println!("Unhandled OSC address: {}", msg.addr);
                                println!("Unhandled OSC arguments: {:?}", msg.args);
                            }
                        }
                    }
                    OscPacket::Bundle(bundle) => {
                        println!("OSC Bundle: {:?}", bundle);
                    }
                }
            }
            Err(e) => {
                println!("Error receiving from socket: {}", e);
                break;
            }
        }
    }

}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();
    println!("{:?}", opt);

    let mut file = File::open(opt.input)?;

    match opt.cmd {
        Command::CreateEditTable(opts) => create_edit_table(&mut file, &opts),
        Command::ApplyEditTable(opts) => apply_edit_table(&mut file, &opts),
        Command::ApplyRandomEdits(opts) => apply_random_edits(&opts),
        Command::QuickMode(opts) => quick_mode(&mut file, &opts),
        Command::Streaming(opts) => streaming_mode(&opts),
    }
}
