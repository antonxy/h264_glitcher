use crate::h264::NALUnitType;
use enum_primitive::FromPrimitive;
use std::io;
use io::{Write, Cursor, SeekFrom};
use bitstream_io::{BigEndian, BitWriter, BitWrite, BitReader, BitRead};

#[derive(Debug)]
pub enum NalParseError {
    EndOfStream,
    InvalidData,
    IoError(io::Error),
    Unimplemented
}

impl From<io::Error> for NalParseError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            Self::EndOfStream
        } else {
            Self::IoError(e)
        }
    }
}

#[derive(Clone)]
pub struct NalUnit {
    pub nal_ref_idc : u8,
    pub nal_unit_type: NALUnitType,
    pub rbsp: Vec<u8>,
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
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NalParseError> {
        let mut reader = BitReader::endian(bytes, BigEndian);

        if reader.read_bit()? != false { //forbidden_zero_bit
            return Err(NalParseError::InvalidData);
        }

        let nal_ref_idc = reader.read(2)?;
        let nal_unit_type = reader.read(5)?;

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

fn read_ue(reader: &mut impl BitRead) -> Result<u32, NalParseError> {
    let mut leading_zero_bits = 0;
    loop {
        if reader.read_bit()? {
            break;
        }
        leading_zero_bits += 1;
    }

    if leading_zero_bits > 32 {
        return Err(NalParseError::Unimplemented);
    }
    let bits = reader.read::<u32>(leading_zero_bits)?;
    Ok((1 << leading_zero_bits) - 1 + bits)
}

fn write_ue(writer: &mut impl BitWrite, value: u32) -> io::Result<()> {
    let leading_zero_bits : u32 = ((value + 1) as f64).log2() as u32;
    writer.write(leading_zero_bits, 0)?;
    writer.write_bit(true)?;
    writer.write(leading_zero_bits, value + 1 - ( 1 << leading_zero_bits ))?;
    Ok(())
}

pub struct SliceHeader {
    pub first_mb_in_slice : u32,
    pub slice_type : u32,
    pub pic_parameter_set_id : u32,
    pub frame_num : u32,

    data: Vec<u8>,
    data_offset: u64,
}

impl SliceHeader {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NalParseError> {
        let mut reader = BitReader::endian(Cursor::new(bytes), BigEndian);
        let first_mb_in_slice = read_ue(&mut reader)?;
        let slice_type = read_ue(&mut reader)?;
        let pic_parameter_set_id = read_ue(&mut reader)?;

        let separate_colour_plane_flag = false; //TODO actually get this from SPS
        if separate_colour_plane_flag {
            let colour_plane_id = reader.read::<u8>(2)?;
        }

        let frame_num_bits = 4; //TODO actually get this from SPS
        let frame_num = reader.read(frame_num_bits)?;

        Ok(Self {
            first_mb_in_slice,
            slice_type,
            pic_parameter_set_id,
            frame_num,
            data : bytes.into(),
            data_offset : reader.position_in_bits()?,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vec = Vec::new();
        let mut writer = BitWriter::endian(&mut vec, BigEndian);

        write_ue(&mut writer, self.first_mb_in_slice).unwrap();
        write_ue(&mut writer, self.slice_type).unwrap();
        write_ue(&mut writer, self.pic_parameter_set_id).unwrap();

        //TODO colour plane

        //TODO variable size
        writer.write(4, self.frame_num).unwrap();

        // TODO this is probably highly inefficient
        let mut reader = BitReader::endian(Cursor::new(&self.data), BigEndian);
        reader.seek_bits(SeekFrom::Start(self.data_offset)).unwrap();
        loop {
            match reader.read_bit() {
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => panic!("Read failed from vec: {}", e),
                Ok(bit) => writer.write_bit(bit).unwrap(),
            }
        }

        writer.byte_align().unwrap();

        vec
    }
}


#[cfg(test)]
mod test {
    use crate::NalIterator;
    use crate::NalUnit;
    use crate::h264::NALUnitType;
    use crate::parse_nal::*;
    use std::io::Read;

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

    #[test]
    fn test_encode_rbsp_to_nal() {
        assert_eq!(encode_rbsp_to_nal(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x01]), &[0x00, 0x01, 0x00, 0x00, 0x03, 0x00, 0x01]);
        assert_eq!(encode_rbsp_to_nal(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), &[0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00, 0x00]);
    }

    #[test]
    fn smoke_test() {
        let file = std::fs::File::open("./big_buck_bunny.h264").unwrap();
        let file = std::io::BufReader::new(file);
        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
        let it = it.map(move |x| NalUnit::from_bytes(&x));
        for unit in it {
            let unit = unit.unwrap();
            match unit.nal_unit_type {
                NALUnitType::CodedSliceIdr | NALUnitType::CodedSliceNonIdr => {
                    let header = SliceHeader::from_bytes(&unit.rbsp).unwrap();
                },
                _ => {},
            }
        }
    }
}
