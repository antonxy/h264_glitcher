use crate::h264::NALUnitType;
use enum_primitive::FromPrimitive;
use bitreader::{BitReader, BitReaderError};

#[derive(Debug)]
pub enum NalParseError {
    EndOfStream,
    InvalidData,
    Unimplemented
}

impl From<BitReaderError> for NalParseError {
    fn from(e: BitReaderError) -> Self {
        match e {
            BitReaderError::NotEnoughData{..} => { Self::EndOfStream },
            BitReaderError::TooManyBitsForType{..} => { panic!("Programming error: {:?}", e) }
        }
    }
}

pub struct NalUnit {
    pub nal_ref_idc : u8,
    pub nal_unit_type: NALUnitType,
    rbsp: Vec<u8>,
}

fn decode_nal_to_rbsp(bytes: &[u8]) -> Vec<u8> {
    let mut rbsp = Vec::with_capacity(bytes.len());

    let mut i = 0;
    while i < bytes.len() {
        if i + 2 < bytes.len() && bytes[i..=i+2] == [0x00, 0x00, 0x03] {
            rbsp.push(bytes[i]);
            i += 1;
            rbsp.push(bytes[i]);
            i += 2;
        } else {
            rbsp.push(bytes[i]);
            i += 1;
        }
    }
    rbsp
}

impl NalUnit {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NalParseError> {
        let mut reader = BitReader::new(bytes);

        if reader.read_bool()? != false { //forbidden_zero_bit
            return Err(NalParseError::InvalidData);
        }

        let nal_ref_idc = reader.read_u8(2)?;
        let nal_unit_type = reader.read_u8(5)?;

        let nal_unit_header_bytes = 1;

        // extensions
        match nal_unit_type {
            14 | 20 | 21 => { return Err(NalParseError::Unimplemented)? },
            _ => {}
        }

        let nal_unit_type = NALUnitType::from_u8(nal_unit_type).ok_or(NalParseError::InvalidData)?;
        let rbsp = decode_nal_to_rbsp(&bytes[nal_unit_header_bytes..]);

        Ok(Self {
            nal_ref_idc,
            nal_unit_type,
            rbsp
        })
    }
}


#[cfg(test)]
mod test {
    use crate::NalIterator;
    use crate::NalUnit;
    use std::io::Read;

    #[test]
    fn smoke_test() {
        let file = std::fs::File::open("./big_buck_bunny.h264").unwrap();
        let file = std::io::BufReader::new(file);
        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
        let it = it.map(move |x| NalUnit::from_bytes(&x));
        let vec : Vec<_> = it.collect();
    }
}
