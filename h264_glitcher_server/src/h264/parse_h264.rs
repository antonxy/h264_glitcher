use bitstream_io::{BitRead, BitWrite};
use std::convert::{TryFrom, TryInto};
use std::io;

#[derive(Debug)]
pub enum ParseError {
    EndOfStream,
    InvalidData,
    IoError(io::Error),
    Unimplemented,
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

pub fn read_ue<U: TryFrom<u32> + From<u8>, R: BitRead>(reader: &mut R) -> Result<U, ParseError> {
    let mut leading_zero_bits = 0;
    loop {
        if reader.read_bit()? {
            break;
        }
        leading_zero_bits += 1;
    }

    if leading_zero_bits > 31 {
        return Err(ParseError::Unimplemented);
    } else if leading_zero_bits > 0 {
        let bits = reader.read::<u32>(leading_zero_bits)?;
        ((1 << leading_zero_bits) - 1 + bits)
            .try_into()
            .map_err(|_| ParseError::InvalidData)
    } else {
        Ok(0.into())
    }
}

fn golomb_to_signed(val: u32) -> i32 {
    let sign = (((val & 0x1) as i32) << 1) - 1;
    ((val >> 1) as i32 + (val & 0x1) as i32) * sign
}

pub fn read_se<S: TryFrom<i32>, R: BitRead>(reader: &mut R) -> Result<S, ParseError> {
    golomb_to_signed(read_ue::<u32, _>(reader)?)
        .try_into()
        .map_err(|_| ParseError::InvalidData)
}

//TODO also make generic and get rid of the ugly f64
pub fn write_ue<W: BitWrite>(writer: &mut W, value: u32) -> io::Result<()> {
    let leading_zero_bits: u32 = ((value + 1) as f64).log2() as u32;
    writer.write(leading_zero_bits, 0)?;
    writer.write_bit(true)?;
    writer.write(leading_zero_bits, value + 1 - (1 << leading_zero_bits))?;
    Ok(())
}

pub fn read_optional<T, R, F>(reader: &mut R, read_contents: F) -> Result<Option<T>, ParseError>
where
    R: BitRead,
    F: FnOnce(&mut R) -> Result<T, ParseError>,
{
    if reader.read_bit()? {
        Ok(Some(read_contents(reader)?))
    } else {
        Ok(None)
    }
}

pub fn read_optional_unimplemented<R>(reader: &mut R) -> Result<(), ParseError>
where
    R: BitRead,
{
    if reader.read_bit()? {
        Err(ParseError::Unimplemented)
    } else {
        Ok(())
    }
}

pub fn write_optional<T, W, F>(writer: &mut W, opt: &Option<T>, write_contents: F) -> io::Result<()>
where
    W: BitWrite,
    F: FnOnce(&mut W, &T) -> io::Result<()>,
{
    writer.write_bit(opt.is_some())?;
    if let Some(val) = opt {
        write_contents(writer, val)?;
    }
    Ok(())
}

pub fn read_rbsp_trailing_bits(reader: &mut impl BitRead) -> Result<(), ParseError> {
    if !reader.read_bit()? {
        return Err(ParseError::InvalidData);
    }
    while !reader.byte_aligned() {
        if reader.read_bit()? {
            return Err(ParseError::InvalidData);
        }
    }
    Ok(())
}

pub fn write_rbsp_trailing_bits(writer: &mut impl BitWrite) -> io::Result<()> {
    writer.write_bit(true)?;
    writer.byte_align()
}

pub fn more_rbsp_data<R: BitRead + Clone>(reader: &mut R) -> Result<bool, ParseError> {
    let mut throwaway = reader.clone();
    let r = (move || {
        throwaway.skip(1)?;
        throwaway.read_unary1()?;
        Ok::<_, std::io::Error>(())
    })();
    match r {
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e.into()),
        Ok(_) => Ok(true),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitstream_io::{BigEndian, BitReader, BitWriter};

    #[test]
    fn read_ue_test() {
        let data: &[u8] = &[0b11101000];
        assert_eq!(
            read_ue::<u32, _>(&mut BitReader::endian(data, BigEndian)).unwrap(),
            0
        ); //1 - 0

        let data: &[u8] = &[0b01001000];
        assert_eq!(
            read_ue::<u32, _>(&mut BitReader::endian(data, BigEndian)).unwrap(),
            1
        ); //010 - 1

        let data: &[u8] = &[0b01101000];
        assert_eq!(
            read_ue::<u32, _>(&mut BitReader::endian(data, BigEndian)).unwrap(),
            2
        ); //011 - 2

        let data: &[u8] = &[0b00001000, 0b10000000];
        assert_eq!(
            read_ue::<u32, _>(&mut BitReader::endian(data, BigEndian)).unwrap(),
            16
        ); //000010001 - 16
    }

    #[test]
    fn write_ue_test() {
        for i in 0..1000 {
            let mut vec = Vec::new();
            let mut writer = BitWriter::endian(&mut vec, BigEndian);
            write_ue(&mut writer, i).unwrap();
            writer.byte_align().unwrap();
            assert_eq!(
                read_ue::<u32, _>(&mut BitReader::endian(vec.as_slice(), BigEndian)).unwrap(),
                i
            );
        }
    }

    #[test]
    fn test_ue_barely_overflow() {
        for i in 0..u8::MAX {
            let mut vec = Vec::new();
            let mut writer = BitWriter::endian(&mut vec, BigEndian);
            write_ue(&mut writer, i as u32).unwrap();
            writer.byte_align().unwrap();
            assert_eq!(
                read_ue::<u8, _>(&mut BitReader::endian(vec.as_slice(), BigEndian)).unwrap(),
                i
            );
        }
    }

    #[test]
    fn test_ue_overflow() {
        for i in u8::MAX as u32 + 1..1000 {
            let mut vec = Vec::new();
            let mut writer = BitWriter::endian(&mut vec, BigEndian);
            write_ue(&mut writer, i).unwrap();
            writer.byte_align().unwrap();
            read_ue::<u8, _>(&mut BitReader::endian(vec.as_slice(), BigEndian)).unwrap_err();
        }
    }
}
