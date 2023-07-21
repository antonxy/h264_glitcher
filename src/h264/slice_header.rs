use crate::h264::{write_ue, read_ue, read_se, ParseError};
use crate::h264::{Sps, Pps, NALUnitType, PicOrderCntType};
use std::io;
use std::fmt;
use io::{Cursor, SeekFrom};
use bitstream_io::{BigEndian, BitWriter, BitWrite, BitReader, BitRead};

#[derive(Clone, Debug)]
pub struct SliceHeader {
    // Section 7.3.3

    pub first_mb_in_slice : u32,
    pub slice_type : u32,
    pub pic_parameter_set_id : u32,
    pub frame_num : u32,
    pub field_pic_flag: bool,
    pub bottom_field_flag: bool,
    pub idr_pic_id: Option<u32>,
    pub redundant_pic_cnt: Option<u32>,
    pub pred_weight_table: Option<PredWeightTable>,

    pub data: Vec<u8>,
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
    // TODO create parsing context containing spss, ppss, ...
    pub fn from_bytes(bytes: &[u8], sps: &Sps, pps: &Pps, nal_unit_type: &NALUnitType, nal_ref_idc: u8) -> Result<Self, ParseError> {
        let mut reader = BitReader::endian(Cursor::new(bytes), BigEndian);
        let first_mb_in_slice = read_ue(&mut reader)?;
        let slice_type = read_ue(&mut reader)?;
        let pic_parameter_set_id = read_ue(&mut reader)?;
        assert!(pic_parameter_set_id == pps.pic_parameter_set_id); // TODO: pass in all known PPSs
        assert!(pps.seq_parameter_set_id == sps.seq_parameter_set_id); // TODO: pass in all known SPSs

        if sps.separate_colour_plane_flag {
            let _colour_plane_id = reader.read::<u8>(2)?;
        }

        let frame_num = reader.read((sps.log2_max_frame_num_minus4 + 4).into())?;

        let mut field_pic_flag = false;
        let mut bottom_field_flag = false;
        if !sps.frame_mbs_only_flag {
            field_pic_flag = reader.read_bit()?;
            if field_pic_flag {
                bottom_field_flag = reader.read_bit()?;
            }
        }

        let idr_pic_id = if nal_unit_type.idr_pic_flag(){
            Some(read_ue(&mut reader)?)
        } else {
            None
        };

        match sps.pic_order_cnt_type {
//            PicOrderCntType::Type0(log2_max_pic_order_cnt_lsb_minus4) => {
//                pic_order_cnt_lsb = reader.read(log2_max_pic_order_cnt_lsb_minus4 + 4)?;
//                if pps.bottom_field_pic_order_in_frame_present_flag && !field_pic_flag {
//                    delta_pic_order_cnt_bottom = read_se(&mut reader)?;
//                }
//            }
            PicOrderCntType::Type2 => {},
            _ => {
                return Err(ParseError::Unimplemented);
            },
        }

        let redundant_pic_cnt = if pps.redundant_pic_cnt_present_flag {
            Some(read_ue(&mut reader)?)
        } else {
            None
        };

        let unparsed_start = reader.position_in_bits()?;

        if slice_type % 5 == 1 { // B Slice
            reader.read_bit()?;
        }
        let mut num_ref_idx_l0_active_minus1 = None;
        if slice_type % 5 == 0 || slice_type % 5 == 3 || slice_type % 5 == 1 { // P, SP, or B Slice
            let flag = reader.read_bit()?;
            if flag {
                num_ref_idx_l0_active_minus1 = Some(read_ue(&mut reader)?);
                if slice_type % 5 == 1 { // B Slice
                    let num_ref_idx_l1_active_minus1 : u32 = read_ue(&mut reader)?;
                }
            }
        }
        match nal_unit_type {
            NALUnitType::CodedSliceSvcExtension | NALUnitType::NAL21 => {
                return Err(ParseError::Unimplemented);
            },
            _ => {
                //ref pic list modification()
            }
        }

        let mut pred_weight_table = None;
        if pps.weighted_pred_flag && (slice_type % 5 == 0 || slice_type % 5 == 3) || // P or SP
            (pps.weighted_bipred_idc == 1 && slice_type % 5 == 1) { // B

            //pred_weight_table() - might be interesting for messing with
            pred_weight_table = Some(PredWeightTable::read(&mut reader, sps, num_ref_idx_l0_active_minus1.unwrap())?);
        }

        if nal_ref_idc != 0 { // from NALUnit
            // dec_ref_pic_marking()
        }

        if pps.entropy_coding_mode_flag && slice_type % 5 != 2 && slice_type % 5 != 4 {
            let cabac_init_idc : u32 = read_ue(&mut reader)?;
        }
        let slice_qp_delta : i32 = read_se(&mut reader)?;

        if slice_type % 5 == 3 || slice_type % 5 == 4 { // SP or SI
            if slice_type % 5 == 3 {
                reader.read_bit()?;
            }
            let _ : i32 = read_se(&mut reader)?;
        }

        if pps.deblocking_filter_control_present_flag {
            let disable_deblocking_filter_idc : u32 = read_ue(&mut reader)?;
            if disable_deblocking_filter_idc != 1 {
                let slice_alpha_c0_offset_div2 : i32 = read_se(&mut reader)?;
                let slice_beta_offset_div2 : i32 = read_se(&mut reader)?;
            }
        }

        //TODO slice_groups

        Ok(Self {
            first_mb_in_slice,
            slice_type,
            pic_parameter_set_id,
            frame_num,
            field_pic_flag,
            bottom_field_flag,
            idr_pic_id,
            redundant_pic_cnt,
            pred_weight_table,

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

        //TODO new fields

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

#[derive(Clone, Debug, PartialEq)]
pub struct PredWeightTable {
}

impl PredWeightTable {
    pub fn read(reader: &mut impl BitRead, sps: &Sps, num_ref_idx_l0_active_minus1: u32) -> Result<Self, ParseError> {
        let luma_log2_weight_denom : u32 = read_ue(reader)?;
        if sps.chroma_array_type() != 0 {
            let chroma_log2_weight_denom : u32 = read_ue(reader)?;
        }

        for i in (0..=num_ref_idx_l0_active_minus1) {
            let flag = reader.read_bit()?;
            if flag {
                let luma_weight_l0 : i32 = read_se(reader)?;
                let luma_offset_l0 : i32 = read_se(reader)?;
            }
            if sps.chroma_array_type() != 0 {
                let flag = reader.read_bit()?;
                if flag {
                    for i in (0..2) {
                        let chroma_weight_l0 : i32 = read_se(reader)?;
                        let chroma_offset_l0 : i32 = read_se(reader)?;
                    }
                }
            }
        }

        //if slice_type % 5 == 1 { // B Slice
        //    return Err(ParseError::Unimplemented);
        //}


        unimplemented!();
        Ok(Self {
        })
    }
    pub fn write(&self, writer: &mut impl BitWrite) -> std::io::Result<()> {
        unimplemented!();
        Ok(())
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
