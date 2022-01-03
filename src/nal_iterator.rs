use std::iter::Iterator;
use itertools::structs::PeekNth;
use itertools::peek_nth;

//Info on byte stream format
//https://yumichan.net/video-processing/video-compression/introduction-to-h264-nal-unit/

//Iterator seems to work but also seems to be highly inefficient

pub struct NalIterator<H264Stream: Iterator<Item = u8>>
{
    stream: PeekNth<H264Stream>,
}

impl<H264Stream> NalIterator<H264Stream>
where
    H264Stream: Iterator<Item = u8>,
{
    pub fn new(stream: H264Stream) -> NalIterator<H264Stream> {
        NalIterator {
            stream: peek_nth(stream),
        }
    }
}

fn peek_n<I: Iterator<Item=u8>>(it: &mut PeekNth<I>, n: usize) -> Vec<u8>
{
    (0..n).map(move |i| it.peek_nth(i).map(|x| *x))
        .take_while(|x| x.is_some())
        .map(|x| x.unwrap())
        .collect()
}

impl<H264Stream> Iterator for NalIterator<H264Stream>
where
    H264Stream: Iterator<Item = u8>,
{
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        // Find nal start (0x000001 or 0x00000001)
        let mut last_len = 0;
        loop {
            if peek_n(&mut self.stream, 3) == [0x00, 0x00, 0x01] {
                if self.stream.nth(2).is_none() { //advance
                    return None
                }
                break;
            }
            if peek_n(&mut self.stream, 4) == [0x00, 0x00, 0x00, 0x01] {
                if self.stream.nth(3).is_none() { //advance
                    return None
                }
                break;
            }
            if self.stream.next().is_none() {
                return None
            }
        }

        let mut nal_data = Vec::<u8>::new();

        // Find nal end (start of next nal) (0x000001 or 0x00000001)
        // Put to puffer while searching
        let mut last_len = 0;
        loop {
            if peek_n(&mut self.stream, 3) == [0x00, 0x00, 0x01] {
                break;
            }
            if peek_n(&mut self.stream, 4) == [0x00, 0x00, 0x00, 0x01] {
                break;
            }
            match self.stream.next() {
                Some(x) => nal_data.push(x),
                None => return None,
            }
        }

        Some(nal_data)
    }
}

#[cfg(test)]
mod test {
    use super::NalIterator;
    use std::io::Read;
    #[test]
    fn test_short_head() {
        let data : Vec<u8> = vec![0xaa, 0xaa, 0x00, 0x00, 0x01, 0xbb, 0xbb, 0xbb, 0x00, 0x00, 0x01];
        let it = NalIterator::new(data.into_iter());
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 1);
        let packet : &[u8] = &[0xbb, 0xbb, 0xbb][..];
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
