use crate::{h264::FrameType, NALUnitType, libh264bitstream};
use std::{convert::TryInto, os::raw::c_int};
use crate::enum_primitive::FromPrimitive;

use crate::libh264bitstream::{h264_stream_t, read_nal_unit};

#[derive(Debug)]
pub struct NalItem {
    pub nal_unit_type: NALUnitType,
    pub frame_type: FrameType,
}

pub struct H264Parser {
    h2 : *mut h264_stream_t,
}

impl H264Parser {

    pub fn new() -> Self {
        let h2: *mut h264_stream_t = unsafe { libh264bitstream::h264_new() };
        Self {
            h2
        }
    }

    pub fn parse_nal<T: AsRef<[u8]>>(&mut self, nal_data : T) -> Option<NalItem> {
        let nal_data : &[u8] = nal_data.as_ref();
        // create NalItem
        let ret = unsafe {
            read_nal_unit(
                self.h2,
                nal_data.to_vec().as_mut_ptr(),
                nal_data.len() as i32
                )
        };
        if ret == -1 || ret != nal_data.len() as i32 {
            // sometimes, read_nal_unit fails
            return None;
        }

        let nal_unit_type = unsafe { (*(*self.h2).nal).nal_unit_type.into() };
        let frame_type = unsafe { FrameType::from_sh_slice_type((*(*self.h2).sh).slice_type) };

        let item = NalItem { nal_unit_type: NALUnitType::from_i32(nal_unit_type).unwrap(), frame_type };

        Some(item)
    }
}


impl Drop for H264Parser {
    fn drop(&mut self) {
        unsafe { libh264bitstream::h264_free(self.h2) };
    }
}

mod test {
    use std::io::Read;
    use crate::parse_nal::H264Parser;
    use crate::nal_iterator::NalIterator;
    #[test]
    fn smoke_test() {
        let file = std::fs::File::open("./big_buck_bunny.h264").unwrap();
        let file = std::io::BufReader::new(file);
        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
        let mut parser = H264Parser::new();
        let it = it.map(move |x| parser.parse_nal(x));
        for el in it {
            println!("{:?}", el);
        }
    }
}
