use crate::h264::write_ue;
use crate::h264::ParseError;
use crate::h264::read_ue;
use std::io;
use std::fmt;
use io::{Cursor, SeekFrom};
use bitstream_io::{BigEndian, BitWriter, BitWrite, BitReader, BitRead};


#[derive(Clone, Debug)]
pub struct SliceHeader {
    pub first_mb_in_slice : u32,
    pub slice_type : u32,
    pub pic_parameter_set_id : u32,
    pub frame_num : u32,

    data: Vec<u8>,
    data_offset: u64,
}


impl fmt::Display for SliceHeader {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("SliceHeader")
            .field("first_mb_in_slice", &self.first_mb_in_slice)
            .field("slice_type", &self.slice_type)
            .field("pic_parameter_set_id", &self.pic_parameter_set_id)
            .field("frame_num", &self.frame_num)
            .finish()
    }
}

impl SliceHeader {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let mut reader = BitReader::endian(Cursor::new(bytes), BigEndian);
        let first_mb_in_slice = read_ue(&mut reader)?;
        let slice_type = read_ue(&mut reader)?;
        let pic_parameter_set_id = read_ue(&mut reader)?;

        let separate_colour_plane_flag = false; //TODO actually get this from SPS
        if separate_colour_plane_flag {
            let _colour_plane_id = reader.read::<u8>(2)?;
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
    use super::*;
    use crate::h264::{NalIterator, NalUnit, NALUnitType};
    use std::io::Read;

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
