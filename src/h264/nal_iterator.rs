use std::iter::Iterator;

//Info on byte stream format
//https://yumichan.net/video-processing/video-compression/introduction-to-h264-nal-unit/

pub struct NalIterator<H264Stream: Iterator<Item = u8>>
{
    stream: H264Stream,
    found_nal_last_time: bool,
}

impl<H264Stream> NalIterator<H264Stream>
where
    H264Stream: Iterator<Item = u8>,
{
    pub fn new(stream: H264Stream) -> NalIterator<H264Stream> {
        NalIterator {
            stream: stream,
            found_nal_last_time: false,
        }
    }
}

fn take_until_nal_start<I : Iterator<Item=u8>>(it: &mut I) -> Option<Vec<u8>> {
    let mut nal_data = Vec::<u8>::new();

    // Find nal end (start of next nal) (0x000001 or 0x00000001)
    // Put to puffer while searching
    let mut zeros_found : i32 = 0;
    loop {
        let next = it.next()?;
        if next == 0x00 {
            zeros_found += 1;
        } else if next == 0x01 {
            if zeros_found >= 2 {
                for _ in 0..(zeros_found-3) {
                    nal_data.push(0x00);
                }
                //found NAL start
                return Some(nal_data);
            } else {
                for _ in 0..zeros_found {
                    nal_data.push(0x00);
                }
                nal_data.push(next);
                zeros_found = 0;
            }
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
        // From then on we can assume the separator has been consumed by the previous invocation.
        if !self.found_nal_last_time {
            take_until_nal_start(&mut self.stream)?;
            self.found_nal_last_time = true;
        }

        take_until_nal_start(&mut self.stream)
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

    #[test]
    fn smoke_test() {
        let file = std::fs::File::open("./big_buck_bunny.h264").unwrap();
        let file = std::io::BufReader::new(file);
        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 787);
    }
}
