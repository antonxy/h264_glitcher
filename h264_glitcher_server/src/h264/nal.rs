use crate::h264::ParseError;
use crate::h264::NALUnitType;
use enum_primitive::FromPrimitive;
use std::fmt;
use bitstream_io::{BigEndian, BitWriter, BitWrite, BitReader, BitRead};


#[derive(Clone, Debug)]
pub struct NalUnit {
    pub nal_ref_idc : u8,
    pub nal_unit_type: NALUnitType,
    pub rbsp: Vec<u8>,
}

impl fmt::Display for NalUnit {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("NalUnit")
            .field("nal_ref_idc", &self.nal_ref_idc)
            .field("nal_unit_type", &self.nal_unit_type)
            .finish()
    }
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

fn encode_rbsp_to_nal(bytes: &[u8]) -> Vec<u8> {
    let mut nal = Vec::with_capacity(bytes.len() * 3 / 2);

    let mut num_zeros = 0;
    for byte in bytes {
        if num_zeros >= 2 {
            if byte <= &0x03 {
                nal.push(0x03);
                num_zeros = 0;
            }
        }
        nal.push(*byte);
        if byte == &0x00 {
            num_zeros += 1;
        } else {
            num_zeros = 0;
        }
    }
    nal
}

impl NalUnit {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let mut reader = BitReader::endian(bytes, BigEndian);

        if reader.read_bit()? != false { //forbidden_zero_bit
            return Err(ParseError::InvalidData);
        }

        let nal_ref_idc = reader.read(2)?;
        let nal_unit_type = reader.read(5)?;

        let nal_unit_header_bytes = 1;

        // extensions
        match nal_unit_type {
            14 | 20 | 21 => { return Err(ParseError::Unimplemented)? },
            _ => {}
        }

        let nal_unit_type = NALUnitType::from_u8(nal_unit_type).ok_or(ParseError::InvalidData)?;
        let rbsp = decode_nal_to_rbsp(&bytes[nal_unit_header_bytes..]);

        Ok(Self {
            nal_ref_idc,
            nal_unit_type,
            rbsp
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vec = Vec::new();
        let mut writer = BitWriter::endian(&mut vec, BigEndian);

        writer.write_bit(false).unwrap();
        writer.write(2, self.nal_ref_idc).unwrap();
        writer.write(5, self.nal_unit_type as u8).unwrap();
        assert!(writer.byte_aligned());
        writer.write_bytes(&encode_rbsp_to_nal(&self.rbsp)).unwrap();
        vec
    }
}



#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_encode_rbsp_to_nal() {
        assert_eq!(encode_rbsp_to_nal(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x01]), &[0x00, 0x01, 0x00, 0x00, 0x03, 0x00, 0x01]);
        assert_eq!(encode_rbsp_to_nal(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), &[0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00, 0x00]);
    }
}
