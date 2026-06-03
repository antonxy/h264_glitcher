use std::iter::Iterator;

//Info on byte stream format
//https://yumichan.net/video-processing/video-compression/introduction-to-h264-nal-unit/

pub struct NalIterator<H264Stream: Iterator<Item = u8>> {
    stream: H264Stream,
    found_nal_last_time: bool,
}

impl<H264Stream> NalIterator<H264Stream>
where
    H264Stream: Iterator<Item = u8>,
{
    pub fn new(stream: H264Stream) -> NalIterator<H264Stream> {
        NalIterator {
            stream,
            found_nal_last_time: false,
        }
    }
}

/// Reads bytes until the next NAL start code (0x000001 or 0x00000001).
/// Returns (bytes_consumed, nal_data) where nal_data contains the NAL content
/// excluding the start code.
fn take_until_nal_start<I: Iterator<Item=u8>>(it: &mut I) -> Option<(usize, Vec<u8>)> {
    let mut bytes_consumed: usize = 0;
    let mut nal_data = Vec::<u8>::new();

    // Find NAL end (start of next NAL) (0x000001 or 0x00000001)
    let mut zeros_found: i32 = 0;
    loop {
        let next = it.next()?;
        bytes_consumed += 1;
        if next == 0x00 {
            zeros_found += 1;
        } else if next == 0x01 && zeros_found >= 2 {
            for _ in 0..(zeros_found-3) {
                nal_data.push(0x00);
            }
            //found NAL start
            return Some((bytes_consumed, nal_data));
        } else {
            for _ in 0..zeros_found {
                nal_data.push(0x00);
            }
            nal_data.push(next);
            zeros_found = 0;
        }
    }
}

impl<H264Stream> Iterator for NalIterator<H264Stream>
where
    H264Stream: Iterator<Item = u8>,
{
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        // Skip until we have consumed the first NAL separator.
        if !self.found_nal_last_time {
            take_until_nal_start(&mut self.stream)?;
            self.found_nal_last_time = true;
        }

        let (_, nal_data) = take_until_nal_start(&mut self.stream)?;
        Some(nal_data)
    }
}


/// NAL offset/length information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NalLocation {
    /// Byte offset of the NAL data start (after the start code)
    pub offset: usize,
    /// Length of the NAL data (excluding start code)
    pub length: usize,
}

/// Iterator that returns byte offsets and lengths of NALs instead of their contents.
/// Useful when you have random access to the underlying data and want to avoid copying.
pub struct NalOffsetIterator<H264Stream: Iterator<Item = u8>>
{
    stream: H264Stream,
    found_nal_last_time: bool,
    current_position: usize,
}

impl<H264Stream> NalOffsetIterator<H264Stream>
where
    H264Stream: Iterator<Item = u8>,
{
    pub fn new(stream: H264Stream) -> NalOffsetIterator<H264Stream> {
        NalOffsetIterator {
            stream: stream,
            found_nal_last_time: false,
            current_position: 0,
        }
    }
}

impl<H264Stream> Iterator for NalOffsetIterator<H264Stream>
where
    H264Stream: Iterator<Item = u8>,
{
    type Item = NalLocation;

    fn next(&mut self) -> Option<Self::Item> {
        // Skip until we have consumed the first NAL separator.
        if !self.found_nal_last_time {
            let (bytes_consumed, _) = take_until_nal_start(&mut self.stream)?;
            self.current_position += bytes_consumed;
            self.found_nal_last_time = true;
        }

        let nal_start = self.current_position;
        let (bytes_consumed, nal_data) = take_until_nal_start(&mut self.stream)?;
        self.current_position += bytes_consumed;

        Some(NalLocation {
            offset: nal_start,
            length: nal_data.len(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::NalIterator;
    use std::io::Read;
    #[test]
    fn test_short_head() {
        let data : Vec<u8> = vec![0xaa, 0xaa, 0x00, 0x00, 0x01, 0xbb, 0x00, 0x01, 0xbb, 0xbb, 0x00, 0x00, 0x01];
        let it = NalIterator::new(data.into_iter());
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 1);
        let packet : &[u8] = &[0xbb, 0x00, 0x01, 0xbb, 0xbb][..];
        assert_eq!(items[0], packet);
    }

    #[test]
    fn test_long_head() {
        let data : Vec<u8> = vec![0xaa, 0xaa, 0x00, 0x00, 0x00, 0x01, 0xbb, 0xbb, 0xbb, 0x00, 0x00, 0x00, 0x01];
        let it = NalIterator::new(data.into_iter());
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 1);
        let packet : &[u8] = &[0xbb, 0xbb, 0xbb][..];
        assert_eq!(items[0], packet);
    }

    #[test]
    fn test_multi_zero() {
        let data : Vec<u8> = vec![0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xbb, 0x00, 0x00, 0xbb, 0xbb, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        let it = NalIterator::new(data.into_iter());
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 1);
        let packet : &[u8] = &[0xbb, 0x00, 0x00, 0xbb, 0xbb, 0x00, 0x00][..];
        assert_eq!(items[0], packet);
    }

    #[test]
    fn test_multiple() {
        let data : Vec<u8>= vec![0xaa, 0xaa, 0x00, 0x00, 0x01, 0xbb, 0xbb, 0xbb, 0x00, 0x00, 0x01, 0xbb, 0xbb, 0xcc, 0x00, 0x00, 0x01];
        let it = NalIterator::new(data.into_iter());
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 2);
        let packet : &[u8] = &[0xbb, 0xbb, 0xbb][..];
        assert_eq!(items[0], packet);
        let packet : &[u8] = &[0xbb, 0xbb, 0xcc][..];
        assert_eq!(items[1], packet);
    }

//    #[test]
//    fn smoke_test() {
//        let file = std::fs::File::open("./big_buck_bunny.h264").unwrap();
//        let file = std::io::BufReader::new(file);
//        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
//        let items: Vec<_> = it.collect();
//        assert_eq!(items.len(), 787);
//    }
}
