use std::io;
use bitstream_io::{BitWrite, BitRead};


#[derive(Debug)]
pub enum ParseError {
    EndOfStream,
    InvalidData,
    IoError(io::Error),
    Unimplemented
}

impl From<io::Error> for ParseError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            Self::EndOfStream
        } else {
            Self::IoError(e)
        }
    }
}


pub fn read_ue(reader: &mut impl BitRead) -> Result<u32, ParseError> {
    let mut leading_zero_bits = 0;
    loop {
        if reader.read_bit()? {
            break;
        }
        leading_zero_bits += 1;
    }

    if leading_zero_bits > 32 {
        return Err(ParseError::Unimplemented);
    }
    let bits = reader.read::<u32>(leading_zero_bits)?;
    Ok((1 << leading_zero_bits) - 1 + bits)
}

pub fn write_ue(writer: &mut impl BitWrite, value: u32) -> io::Result<()> {
    let leading_zero_bits : u32 = ((value + 1) as f64).log2() as u32;
    writer.write(leading_zero_bits, 0)?;
    writer.write_bit(true)?;
    writer.write(leading_zero_bits, value + 1 - ( 1 << leading_zero_bits ))?;
    Ok(())
}


#[cfg(test)]
mod test {
    use super::*;
    use bitstream_io::{BitReader, BigEndian, BitWriter};

    #[test]
    fn read_ue_test() {
        let data : &[u8] = &[0b11101000];
        assert_eq!(read_ue(&mut BitReader::endian(data, BigEndian)).unwrap(), 0); //1 - 0

        let data : &[u8] = &[0b01001000];
        assert_eq!(read_ue(&mut BitReader::endian(data, BigEndian)).unwrap(), 1); //010 - 1

        let data : &[u8] = &[0b01101000];
        assert_eq!(read_ue(&mut BitReader::endian(data, BigEndian)).unwrap(), 2); //011 - 2

        let data : &[u8] = &[0b00001000, 0b10000000];
        assert_eq!(read_ue(&mut BitReader::endian(data, BigEndian)).unwrap(), 16); //000010001 - 16
    }

    #[test]
    fn write_ue_test() {
        for i in 0..1000 {
            let mut vec = Vec::new();
            let mut writer = BitWriter::endian(&mut vec, BigEndian);
            write_ue(&mut writer, i).unwrap();
            writer.byte_align().unwrap();
            assert_eq!(read_ue(&mut BitReader::endian(vec.as_slice(), BigEndian)).unwrap(), i);
        }
    }
}
