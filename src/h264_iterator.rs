use crate::{h264::FrameType, libh264bitstream};
use std::{convert::TryInto, os::raw::c_int};

use crate::libh264bitstream::{find_nal_unit, h264_stream_t, read_nal_unit};

struct NalIterator<H264Stream: std::io::BufRead> {
    stream: H264Stream,
}

#[derive(Debug)]
struct NalItem {
    frame_type: FrameType,
}

impl<H264Stream> NalIterator<H264Stream>
where
    H264Stream: std::io::BufRead,
{
    pub fn new(stream: H264Stream) -> Self {
        NalIterator { stream }
    }
}

impl<H264Stream> Iterator for NalIterator<H264Stream>
where
    H264Stream: std::io::BufRead,
{
    type Item = std::io::Result<NalItem>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let buf = match self.stream.fill_buf() {
                Ok(buf) => buf,
                Err(e) => return Some(Err(e)),
            };

            // library wants a mut pointer :/
            let mut v = Vec::new();
            v.clone_from_slice(buf);
            let mut buf = v;

            let (nal_start, nal_end) = unsafe {
                let mut nal_start: c_int = 0;
                let mut nal_end: c_int = 0;
                let ret = find_nal_unit(
                    buf.as_mut_ptr(),
                    buf.len().try_into().unwrap(),
                    &mut nal_start,
                    &mut nal_end,
                );
                if ret != 0 {
                    continue; // fill_buf will read more
                }
                (nal_start, nal_end)
            };

            // create NalItem
            let h2: *mut h264_stream_t = unsafe { libh264bitstream::h264_new() };
            let ret = unsafe {
                read_nal_unit(
                    h2,
                    buf.as_mut_ptr().add(nal_start.try_into().unwrap()),
                    nal_end - nal_start,
                )
            };
            assert_eq!(ret, 0);

            let frame_type = unsafe { FrameType::from_sh_slice_type((*(*h2).sh).slice_type) };

            let item = Some(Ok(NalItem { frame_type }));

            unsafe { libh264bitstream::h264_free(h2) };

            // consume the space
            self.stream.consume(nal_end.try_into().unwrap());

            return item;
        }
    }
}

#[cfg(test)]
mod test {
    use super::NalIterator;

    #[test]
    fn smoke_test() {
        let file = std::fs::File::open("./big_buck_bunny.h264").unwrap();
        let file = std::io::BufReader::new(file);
        let it = NalIterator::new(file);
        let items: Vec<_> = it.collect();
        println!("{:?}", items);
        assert_eq!(items.len(), 1);
    }
}
