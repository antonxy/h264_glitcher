use std::io::BufReader;

//Info on byte stream format
//https://yumichan.net/video-processing/video-compression/introduction-to-h264-nal-unit/

//Maybe better take u8 Iterator instead of BufRead? is there a peekable iterator with more than one
//byte lookahead? Is there a buffered version of Read::bytes() ?
//use itertools multipeek or similar

pub struct NalIterator<H264Stream: std::io::BufRead> {
    stream: H264Stream,
}

impl<H264Stream> NalIterator<H264Stream>
where
    H264Stream: std::io::BufRead,
{
    pub fn new(stream: H264Stream) -> NalIterator<BufReader<H264Stream>> {
        NalIterator {
            stream: BufReader::new(stream),
        }
    }
}

impl<H264Stream> Iterator for NalIterator<H264Stream>
where
    H264Stream: std::io::BufRead,
{
    type Item = std::io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        // Find nal start (0x000001 or 0x00000001)
        let mut last_len = 0;
        loop {
            let buf = match self.stream.fill_buf() {
                Ok(buf) => buf,
                Err(e) => return Some(Err(e)),
            };

            //No start found in remaining bytes and no more could be read
            if buf.len() == last_len {
                //return None
            }
            last_len = buf.len();

            //EOF
            if buf.len() == 0 {
                return None;
            }

            if buf.len() < 3 {
                println!("buf < 3");
                continue;
            }
            if buf[0..3] == [0x00, 0x00, 0x01] {
                self.stream.consume(3);
                break;
            }
            if buf.len() < 4 {
                println!("buf < 4");
                continue;
            }
            if buf[0..4] == [0x00, 0x00, 0x00, 0x01] {
                self.stream.consume(4);
                break;
            }
            self.stream.consume(1);
        }

        let mut nal_data = Vec::new();

        // Find nal end (start of next nal) (0x000001 or 0x00000001)
        // Put to puffer while searching
        let mut last_len = 0;
        loop {
            let buf = match self.stream.fill_buf() {
                Ok(buf) => buf,
                Err(e) => return Some(Err(e)),
            };
            //
            //No start found in remaining bytes and no more could be read
            if buf.len() == last_len {
                //return None
            }
            last_len = buf.len();
            dbg!(buf.len());

            //EOF
            if buf.len() == 0 {
                return None;
            }

            if buf.len() < 3 {
                println!("buf2 < 3");
                continue;
            }
            if buf[0..3] == [0x00, 0x00, 0x01] {
                break;
            }
            if buf.len() < 4 {
                println!("buf2 < 4");
                continue;
            }
            if buf[0..4] == [0x00, 0x00, 0x00, 0x01] {
                break;
            }
            nal_data.push(buf[0]);
            self.stream.consume(1);
        }

        Some(Ok(nal_data))
    }
}

#[cfg(test)]
mod test {
    use super::NalIterator;

    #[test]
    fn test_short_head() {
        let data : &[u8] = &[0xaa, 0xaa, 0x00, 0x00, 0x01, 0xbb, 0xbb, 0xbb, 0x00, 0x00, 0x01][..];
        let it = NalIterator::new(data);
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 1);
        let packet : &[u8] = &[0xbb, 0xbb, 0xbb][..];
        assert_eq!(items[0].as_ref().unwrap(), packet);
    }

    #[test]
    fn test_long_head() {
        let data : &[u8] = &[0xaa, 0xaa, 0x00, 0x00, 0x00, 0x01, 0xbb, 0xbb, 0xbb, 0x00, 0x00, 0x00, 0x01][..];
        let it = NalIterator::new(data);
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 1);
        let packet : &[u8] = &[0xbb, 0xbb, 0xbb][..];
        assert_eq!(items[0].as_ref().unwrap(), packet);
    }

    #[test]
    fn test_multiple() {
        let data : &[u8] = &[0xaa, 0xaa, 0x00, 0x00, 0x01, 0xbb, 0xbb, 0xbb, 0x00, 0x00, 0x01, 0xbb, 0xbb, 0xcc, 0x00, 0x00, 0x01][..];
        let it = NalIterator::new(data);
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 2);
        let packet : &[u8] = &[0xbb, 0xbb, 0xbb][..];
        assert_eq!(items[0].as_ref().unwrap(), packet);
        let packet : &[u8] = &[0xbb, 0xbb, 0xcc][..];
        assert_eq!(items[1].as_ref().unwrap(), packet);
    }

    #[test]
    fn smoke_test() {
        let file = std::fs::File::open("./big_buck_bunny.h264").unwrap();
        let file = std::io::BufReader::new(file);
        let it = NalIterator::new(file);
        let items: Vec<_> = it.collect();
        assert_eq!(items.len(), 700);
    }
}
