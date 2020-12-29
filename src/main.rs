extern crate libh264bitstream_sys;
extern crate enum_primitive;
extern crate rand;
extern crate structopt;

use libh264bitstream_sys::*;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::os::raw::c_int;
use std::convert::TryInto;
use enum_primitive::*;
use structopt::StructOpt;

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

    #[structopt(short, long, parse(from_os_str))]
    output: Option<PathBuf>,

    #[structopt(short="p", long, default_value="0.0")]
    iframe_prob: f32,

    #[structopt(short, long)]
    frames: Option<usize>,

    #[structopt(short, long)]
    edit_table_out: Option<PathBuf>,

    #[structopt(short="d", long)]
    edit_table_in: Option<PathBuf>,
}

#[derive(Debug)]
struct EditTableEntry {
    nal_start: usize,
    nal_end: usize,
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();
    println!("{:?}", opt);

    let mut file = File::open(opt.input)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let mut outfile = opt.output.map(|p| File::create(p).unwrap());
    unsafe {
        let h2 = h264_new();
        let mut start_offset : usize = 0;
        let mut i = 0;
        let mut iframes = 0;
        while start_offset < buf.len() && opt.frames.map_or(true, |f| i < f) {
            let mut nal_start : c_int = 0;
            let mut nal_end : c_int = 0;
            find_nal_unit(buf.as_mut_ptr().add(start_offset), (buf.len() - start_offset).try_into().unwrap(), &mut nal_start, &mut nal_end);
            let nal_start_glob = nal_start + start_offset as i32;
            let nal_end_glob = nal_end + start_offset as i32;
            let nal_size = nal_end - nal_start;
            //println!("NAL unit 0x{:x} - 0x{:x} | 0x{:x}", nal_start_glob, nal_end_glob, nal_end - nal_start);
            read_nal_unit(h2, buf.as_mut_ptr().add(nal_start_glob.try_into().unwrap()), nal_end - nal_start);
            //println!("NAL {:?}", *(*h2).nal);
            //println!("SH {:?}", (*(*h2).sh).slice_type);
            //println!("Frame {:?}", (*(*h2).sh).frame_num);
            //println!("FrameType {:?}", FrameType::from_sh_slice_type((*(*h2).sh).slice_type));
            //println!("NAL Unit Type {:?}", NALUnitType::from_i32((*(*h2).nal).nal_unit_type).unwrap());

            let frame_type = FrameType::from_sh_slice_type((*(*h2).sh).slice_type);
            let nal_unit_type = NALUnitType::from_i32((*(*h2).nal).nal_unit_type).unwrap();

            if frame_type == FrameType::IOnly {
                iframes += 1;
            }

            let delete_frame = (frame_type == FrameType::IOnly && iframes >= 2 && (rand::random::<f32>() > opt.iframe_prob));
                //|| (frame_type == FrameType::BOnly);

            if !delete_frame {
                outfile.as_mut().map(|f| f.write_all(&buf[(nal_start_glob as usize - 4)..(nal_end_glob as usize)]).unwrap());
            }

            println!("Frame {:?} - FrameType {:?} - NAL Unit Type {:?} - NAL Size {:} - Delete Frame {:}",
                     (*(*h2).sh).frame_num,
                     frame_type,
                     nal_unit_type,
                     nal_size,
                     delete_frame
                     );

            start_offset += nal_end as usize;
            i += 1;
        }
        h264_free(h2);
    }

    Ok(())
}
