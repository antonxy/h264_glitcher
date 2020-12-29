extern crate libh264bitstream_sys;
extern crate enum_primitive;
extern crate rand;
extern crate structopt;

use libh264bitstream_sys::*;
use std::fs::File;
use std::io::{Read, Write, BufReader, BufRead};
use std::path::PathBuf;
use std::os::raw::c_int;
use std::convert::TryInto;
use enum_primitive::*;
use structopt::StructOpt;
use std::str::FromStr;

#[derive(Debug, Copy, Clone, PartialEq)]
enum FrameType {
    P,
    B,
    I,
    POnly,
    BOnly,
    IOnly,
    SPOnly,
    SIOnly,
}

impl FrameType {
    fn from_sh_slice_type(sh_slice_type : c_int) -> FrameType {
        use FrameType::*;
        match sh_slice_type {
            0 => P,
            1 => B,
            2 => I,
            5 => POnly,
            6 => BOnly,
            7 => IOnly,
            8 => SPOnly,
            9 => SIOnly,
            _ => panic!("not impl")
        }
    }
}

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

    #[structopt(short="o", long, parse(from_os_str))]
    edit_table_out: PathBuf,

    #[structopt(short="p", long, default_value="0.0")]
    iframe_prob: f32,
}

#[derive(Debug)]
struct EditTableEntry {
    nal_start: usize,
    nal_end: usize,
    iframe: bool,
}

impl EditTableEntry {
    fn to_string(&self) -> String {
        format!("0x{:06x}-0x{:06x}-{:5}", self.nal_start, self.nal_end, self.iframe)
    }
    fn from_string(string: &str) -> EditTableEntry {
        let mut it = string.split("-");
        let start_str = it.next().unwrap();
        let end_str = it.next().unwrap();
        let iframe_str = it.next().unwrap();
        let parse_hex = |s : &str| usize::from_str_radix(s.trim().trim_start_matches("0x"), 16).unwrap();
        let parse_bool = |s : &str| bool::from_str(s.trim()).unwrap();
        EditTableEntry {
            nal_start: parse_hex(start_str),
            nal_end: parse_hex(end_str),
            iframe: parse_bool(iframe_str),
        }
    }
}

fn create_edit_table(input_file : &mut File, opt: & CreateEditTableCmd) -> std::io::Result<()> {
    let mut buf = Vec::new();
    input_file.read_to_end(&mut buf)?;

    let mut edit_table_out = File::create(&opt.edit_table_out)?;
    unsafe {
        let h2 = h264_new();
        let mut start_offset : usize = 0;
        let mut i = 0;
        while start_offset < buf.len() && opt.frames.map_or(true, |f| i < f) {
            let mut nal_start : c_int = 0;
            let mut nal_end : c_int = 0;
            find_nal_unit(buf.as_mut_ptr().add(start_offset), (buf.len() - start_offset).try_into().unwrap(), &mut nal_start, &mut nal_end);
            let nal_start_glob = nal_start + start_offset as i32;
            let nal_end_glob = nal_end + start_offset as i32;
            let nal_size = nal_end - nal_start;
            read_nal_unit(h2, buf.as_mut_ptr().add(nal_start_glob.try_into().unwrap()), nal_end - nal_start);

            let frame_type = FrameType::from_sh_slice_type((*(*h2).sh).slice_type);
            let nal_unit_type = NALUnitType::from_i32((*(*h2).nal).nal_unit_type).unwrap();

            let entry = EditTableEntry {
                nal_start: nal_start_glob as usize,
                nal_end: nal_end_glob as usize,
                iframe: frame_type == FrameType::IOnly,
            };

            edit_table_out.write_all(
                format!("{} | dec {:06} | Type {:?}\n",
                        entry.to_string(),
                        nal_start_glob,
                        frame_type
                       ).as_bytes())?;

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

fn apply_edit_table(input_file : &mut File, opt: & ApplyEditTableCmd) -> std::io::Result<()> {
    let mut buf = Vec::new();
    input_file.read_to_end(&mut buf)?;

    let edit_table_file = File::open(&opt.edit_table_in)?;
    let mut edit_table_reader = BufReader::new(edit_table_file);

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

fn apply_random_edits(opt: & ApplyRandomEditsCmd) -> std::io::Result<()> {
    let edit_table_file = File::open(&opt.edit_table_in)?;
    let mut edit_table_reader = BufReader::new(edit_table_file);

    let mut edit_table_out = File::create(&opt.edit_table_out)?;
    let mut num_iframes = 0;
    for line in edit_table_reader.lines().map(|l| l.unwrap()) {
        if line.starts_with("#") {
            continue;
        }
        let entry_str = line.split("|").next().unwrap();
        let entry = EditTableEntry::from_string(entry_str);
        let delete_line = entry.iframe && rand::random::<f32>() > opt.iframe_prob && num_iframes > 1;
        if delete_line {
            edit_table_out.write_all("#".as_bytes())?;
        }
        edit_table_out.write_all(format!("{}\n", line).as_bytes())?;

        if entry.iframe {
            num_iframes += 1;
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();
    println!("{:?}", opt);

    let mut file = File::open(opt.input)?;

    match opt.cmd {
        Command::CreateEditTable(opts) => create_edit_table(&mut file, &opts),
        Command::ApplyEditTable(opts) => apply_edit_table(&mut file, &opts),
        Command::ApplyRandomEdits(opts) => apply_random_edits(&opts),
    }

}
