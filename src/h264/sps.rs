use crate::h264::{
    read_optional, read_optional_unimplemented, read_rbsp_trailing_bits, read_ue, write_optional,
    write_rbsp_trailing_bits, write_ue, ParseError,
};
use bitstream_io::{BitRead, BitWrite};
use visit_diff::Diff;

#[derive(Clone, Debug, Diff, PartialEq)]
pub enum PicOrderCntType {
    Type0(u8), // log2_max_pic_order_cnt_lsb_minus4// 0 to 12
    //Type1, //unimplemented
    Type2,
}

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct Sps {
    pub profile_idc: u8,
    pub constraint_set0_flag: bool,
    pub constraint_set1_flag: bool,
    pub constraint_set2_flag: bool,
    pub constraint_set3_flag: bool,
    pub constraint_set4_flag: bool,
    pub constraint_set5_flag: bool,
    pub level_idc: u8,
    pub seq_parameter_set_id: u8, //can be 0 to 31
    pub chroma_format_idc: u8,    // 0 to 4, default is 1
    pub separate_colour_plane_flag: bool,
    pub bit_depth_luma_minus8: u8,   // 0 to 6, default is 0
    pub bit_depth_chroma_minus8: u8, // 0 to 6, default is 0
    pub qpprime_y_zero_transform_bypass_flag: bool,
    //pub seq_scaling_matrix_present_flag: bool, //unimplemented
    //pub seq_scaling_list_present_flag: Vec<bool>,
    pub log2_max_frame_num_minus4: u8, // 0 to 12
    pub pic_order_cnt_type: PicOrderCntType,
    //pub log2_max_pic_order_cnt_lsb_minus4 : u8, // 0 to 12
    //pub delta_pic_order_always_zero_flag: , //unimplemented
    //pub offset_for_non_ref_pic: ,
    //pub offset_for_top_to_bottom_field:,
    //pub offset_for_ref_frame: Vec<>,
    pub max_num_ref_frames: u32, // 0 to MaxDpbFrames (as specified in clause A.3.1 or A.3.2)
    pub gaps_in_frame_num_value_allowed_flag: bool,
    pub pic_width_in_mbs_minus1: u32,
    pub pic_height_in_map_units_minus1: u32,
    pub frame_mbs_only_flag: bool,
    pub mb_adaptive_frame_field_flag: bool,
    pub direct_8x8_inference_flag: bool,
    pub frame_crop_offset: Option<(u32, u32, u32, u32)>, // left, right, top, bottom
    pub vui_parameters: Option<VuiParameters>,           // Annex E
}

impl Sps {
    pub fn read(reader: &mut impl BitRead) -> Result<Self, ParseError> {
        let profile_idc = reader.read(8)?;
        let constraint_set0_flag = reader.read_bit()?;
        let constraint_set1_flag = reader.read_bit()?;
        let constraint_set2_flag = reader.read_bit()?;
        let constraint_set3_flag = reader.read_bit()?;
        let constraint_set4_flag = reader.read_bit()?;
        let constraint_set5_flag = reader.read_bit()?;

        // reserved_zero_2bits - shall be ignored by the decoder
        reader.read::<u8>(2)?;

        let level_idc = reader.read(8)?;
        let seq_parameter_set_id = read_ue(reader)?;

        let mut chroma_format_idc: u8 = 1;
        let mut separate_colour_plane_flag = false;
        let mut bit_depth_luma_minus8: u8 = 0;
        let mut bit_depth_chroma_minus8: u8 = 0;
        let mut qpprime_y_zero_transform_bypass_flag = false;

        match profile_idc {
            100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135 => {
                chroma_format_idc = read_ue(reader)?;
                if chroma_format_idc == 3 {
                    separate_colour_plane_flag = reader.read_bit()?;
                }
                bit_depth_luma_minus8 = read_ue(reader)?;
                bit_depth_chroma_minus8 = read_ue(reader)?;
                qpprime_y_zero_transform_bypass_flag = reader.read_bit()?;

                // seq_scaling_matrix_present_flag
                read_optional_unimplemented(reader)?;
            }
            _ => {}
        }

        let log2_max_frame_num_minus4: u8 = read_ue(reader)?;
        let pic_order_cnt_type = read_ue(reader)?;
        let pic_order_cnt_type = match pic_order_cnt_type {
            0 => PicOrderCntType::Type0(read_ue(reader)?),
            1 => return Err(ParseError::Unimplemented),
            2 => PicOrderCntType::Type2,
            _ => return Err(ParseError::InvalidData),
        };
        let max_num_ref_frames: u32 = read_ue(reader)?;
        let gaps_in_frame_num_value_allowed_flag: bool = reader.read_bit()?;
        let pic_width_in_mbs_minus1: u32 = read_ue(reader)?;
        let pic_height_in_map_units_minus1: u32 = read_ue(reader)?;
        let frame_mbs_only_flag: bool = reader.read_bit()?;
        let mut mb_adaptive_frame_field_flag: bool = false;
        if !frame_mbs_only_flag {
            mb_adaptive_frame_field_flag = reader.read_bit()?;
        }
        let direct_8x8_inference_flag: bool = reader.read_bit()?;

        let frame_crop_offset = read_optional(reader, |r| {
            Ok((read_ue(r)?, read_ue(r)?, read_ue(r)?, read_ue(r)?))
        })?;

        let vui_parameters = read_optional(reader, |r| VuiParameters::read(r))?;

        read_rbsp_trailing_bits(reader)?;

        Ok(Self {
            profile_idc,
            constraint_set0_flag,
            constraint_set1_flag,
            constraint_set2_flag,
            constraint_set3_flag,
            constraint_set4_flag,
            constraint_set5_flag,
            level_idc,
            seq_parameter_set_id,
            chroma_format_idc,
            separate_colour_plane_flag,
            bit_depth_luma_minus8,
            bit_depth_chroma_minus8,
            qpprime_y_zero_transform_bypass_flag,
            //seq_scaling_matrix_present_flag,
            //pub seq_scaling_list_present_flag,
            log2_max_frame_num_minus4,
            pic_order_cnt_type,
            max_num_ref_frames,
            gaps_in_frame_num_value_allowed_flag,
            pic_width_in_mbs_minus1,
            pic_height_in_map_units_minus1,
            frame_mbs_only_flag,
            mb_adaptive_frame_field_flag,
            direct_8x8_inference_flag,
            frame_crop_offset,
            vui_parameters,
        })
    }

    pub fn write(&self, writer: &mut impl BitWrite) -> std::io::Result<()> {
        writer.write(8, self.profile_idc)?;

        writer.write_bit(self.constraint_set0_flag)?;
        writer.write_bit(self.constraint_set1_flag)?;
        writer.write_bit(self.constraint_set2_flag)?;
        writer.write_bit(self.constraint_set3_flag)?;
        writer.write_bit(self.constraint_set4_flag)?;
        writer.write_bit(self.constraint_set5_flag)?;

        // reserved_zero_2bits
        writer.write::<u8>(2, 0)?;

        writer.write(8, self.level_idc)?;
        write_ue(writer, self.seq_parameter_set_id.into())?;

        match self.profile_idc {
            100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135 => {
                write_ue(writer, self.chroma_format_idc.into())?;
                if self.chroma_format_idc == 3 {
                    writer.write_bit(self.separate_colour_plane_flag)?;
                }
                write_ue(writer, self.bit_depth_chroma_minus8.into())?;
                write_ue(writer, self.bit_depth_chroma_minus8.into())?;
                writer.write_bit(self.qpprime_y_zero_transform_bypass_flag)?;

                // seq_scaling_matrix_present_flag
                writer.write_bit(false)?;
            }
            _ => {}
        }

        write_ue(writer, self.log2_max_frame_num_minus4.into())?;
        write_ue(
            writer,
            match self.pic_order_cnt_type {
                PicOrderCntType::Type0(_) => 0,
                PicOrderCntType::Type2 => 2,
            },
        )?;
        if let PicOrderCntType::Type0(log2_max_pic_order_cnt_lsb_minus4) = self.pic_order_cnt_type {
            write_ue(writer, log2_max_pic_order_cnt_lsb_minus4.into())?;
        }
        write_ue(writer, self.max_num_ref_frames)?;
        writer.write_bit(self.gaps_in_frame_num_value_allowed_flag)?;
        write_ue(writer, self.pic_width_in_mbs_minus1)?;
        write_ue(writer, self.pic_height_in_map_units_minus1)?;
        writer.write_bit(self.frame_mbs_only_flag)?;
        if !self.frame_mbs_only_flag {
            writer.write_bit(self.mb_adaptive_frame_field_flag)?;
        }
        writer.write_bit(self.direct_8x8_inference_flag)?;

        if let Some((l, r, t, b)) = self.frame_crop_offset {
            writer.write_bit(true)?;
            write_ue(writer, l)?;
            write_ue(writer, r)?;
            write_ue(writer, t)?;
            write_ue(writer, b)?;
        } else {
            writer.write_bit(false)?;
        }

        write_optional(writer, &self.vui_parameters, |w, v| v.write(w))?;

        write_rbsp_trailing_bits(writer)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct VideoSignalType {
    pub video_format: u8,
    pub video_full_range_flag: bool,
    pub colour_description: Option<ColourDescription>,
}

impl VideoSignalType {
    pub fn read(reader: &mut impl BitRead) -> Result<Self, ParseError> {
        let video_format = reader.read(3)?;
        let video_full_range_flag = reader.read_bit()?;
        let colour_description = read_optional(reader, |r| ColourDescription::read(r))?;

        Ok(Self {
            video_format,
            video_full_range_flag,
            colour_description,
        })
    }
    pub fn write(&self, writer: &mut impl BitWrite) -> std::io::Result<()> {
        writer.write(3, self.video_format)?;
        writer.write_bit(self.video_full_range_flag)?;
        write_optional(writer, &self.colour_description, |w, v| v.write(w))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct ColourDescription {
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
}

impl ColourDescription {
    pub fn read(reader: &mut impl BitRead) -> Result<Self, ParseError> {
        let colour_primaries = reader.read(8)?;
        let transfer_characteristics = reader.read(8)?;
        let matrix_coefficients = reader.read(8)?;

        Ok(Self {
            colour_primaries,
            transfer_characteristics,
            matrix_coefficients,
        })
    }
    pub fn write(&self, writer: &mut impl BitWrite) -> std::io::Result<()> {
        writer.write(8, self.colour_primaries)?;
        writer.write(8, self.transfer_characteristics)?;
        writer.write(8, self.matrix_coefficients)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct ChromaLocInfo {
    pub chroma_sample_loc_type_top_field: u32,
    pub chroma_sample_loc_type_bottom_field: u32,
}

impl ChromaLocInfo {
    pub fn read(reader: &mut impl BitRead) -> Result<Self, ParseError> {
        let chroma_sample_loc_type_top_field = read_ue(reader)?;
        let chroma_sample_loc_type_bottom_field = read_ue(reader)?;

        Ok(Self {
            chroma_sample_loc_type_top_field,
            chroma_sample_loc_type_bottom_field,
        })
    }
    pub fn write(&self, writer: &mut impl BitWrite) -> std::io::Result<()> {
        write_ue(writer, self.chroma_sample_loc_type_top_field)?;
        write_ue(writer, self.chroma_sample_loc_type_bottom_field)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct TimingInfo {
    pub num_units_in_tick: u32,
    pub time_scale: u32,
    pub fixed_frame_rate_flag: bool,
}

impl TimingInfo {
    pub fn read(reader: &mut impl BitRead) -> Result<Self, ParseError> {
        let num_units_in_tick = reader.read(32)?;
        let time_scale = reader.read(32)?;
        let fixed_frame_rate_flag = reader.read_bit()?;

        Ok(Self {
            num_units_in_tick,
            time_scale,
            fixed_frame_rate_flag,
        })
    }
    pub fn write(&self, writer: &mut impl BitWrite) -> std::io::Result<()> {
        writer.write(32, self.num_units_in_tick)?;
        writer.write(32, self.time_scale)?;
        writer.write_bit(self.fixed_frame_rate_flag)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct BitstreamRestriction {
    pub motion_vectors_over_pic_boundaries_flag: bool,
    pub max_bytes_per_pic_denom: u32,
    pub max_bits_per_mb_denom: u32,
    pub log2_max_mv_length_horizontal: u32,
    pub log2_max_mv_length_vertical: u32,
    pub max_num_reorder_frames: u32,
    pub max_dec_frame_buffering: u32,
}

impl BitstreamRestriction {
    pub fn read(reader: &mut impl BitRead) -> Result<Self, ParseError> {
        let motion_vectors_over_pic_boundaries_flag = reader.read_bit()?;
        let max_bytes_per_pic_denom = read_ue(reader)?;
        let max_bits_per_mb_denom = read_ue(reader)?;
        let log2_max_mv_length_horizontal = read_ue(reader)?;
        let log2_max_mv_length_vertical = read_ue(reader)?;
        let max_num_reorder_frames = read_ue(reader)?;
        let max_dec_frame_buffering = read_ue(reader)?;

        Ok(Self {
            motion_vectors_over_pic_boundaries_flag,
            max_bytes_per_pic_denom,
            max_bits_per_mb_denom,
            log2_max_mv_length_horizontal,
            log2_max_mv_length_vertical,
            max_num_reorder_frames,
            max_dec_frame_buffering,
        })
    }
    pub fn write(&self, writer: &mut impl BitWrite) -> std::io::Result<()> {
        writer.write_bit(self.motion_vectors_over_pic_boundaries_flag)?;
        write_ue(writer, self.max_bytes_per_pic_denom)?;
        write_ue(writer, self.max_bits_per_mb_denom)?;
        write_ue(writer, self.log2_max_mv_length_horizontal)?;
        write_ue(writer, self.log2_max_mv_length_vertical)?;
        write_ue(writer, self.max_num_reorder_frames)?;
        write_ue(writer, self.max_dec_frame_buffering)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct VuiParameters {
    pub aspect_ratio_idc: Option<u8>,
    pub overscan_appropriate_flag: Option<bool>,
    pub video_signal_type: Option<VideoSignalType>,
    pub chroma_loc_info: Option<ChromaLocInfo>,
    pub timing_info: Option<TimingInfo>,
    pub pic_struct_present_flag: bool,
    pub bitstream_restriction: Option<BitstreamRestriction>,
}

impl VuiParameters {
    pub fn read(reader: &mut impl BitRead) -> Result<Self, ParseError> {
        let aspect_ratio_idc = read_optional(reader, |r| {
            let idc = r.read::<u8>(8)?;
            if idc == 255 {
                return Err(ParseError::Unimplemented); // Extended_SAR
            }
            Ok(idc)
        })?;

        let overscan_appropriate_flag = read_optional(reader, |r| Ok(r.read_bit()?))?;
        let video_signal_type = read_optional(reader, |r| VideoSignalType::read(r))?;
        let chroma_loc_info = read_optional(reader, |r| ChromaLocInfo::read(r))?;
        let timing_info = read_optional(reader, |r| TimingInfo::read(r))?;

        //TODO
        //nal_hrd_parameters_present_flag
        read_optional_unimplemented(reader)?;

        //vcl_hrd_parameters_present_flag
        read_optional_unimplemented(reader)?;

        //if( nal_hrd_parameters_present_flag || vcl_hrd_parameters_present_flag ) low_delay_hrd_flag

        let pic_struct_present_flag = reader.read_bit()?;
        let bitstream_restriction = read_optional(reader, |r| BitstreamRestriction::read(r))?;

        Ok(Self {
            aspect_ratio_idc,
            overscan_appropriate_flag,
            video_signal_type,
            chroma_loc_info,
            timing_info,
            pic_struct_present_flag,
            bitstream_restriction,
        })
    }
    pub fn write(&self, writer: &mut impl BitWrite) -> std::io::Result<()> {
        write_optional(writer, &self.aspect_ratio_idc, |w, aspect| {
            if *aspect == 255 {
                unimplemented!(); // Extended_SAR
            }
            w.write(8, *aspect)
        })?;

        write_optional(writer, &self.overscan_appropriate_flag, |w, v| {
            w.write_bit(*v)
        })?;

        write_optional(writer, &self.video_signal_type, |w, v| v.write(w))?;
        write_optional(writer, &self.chroma_loc_info, |w, v| v.write(w))?;
        write_optional(writer, &self.timing_info, |w, v| v.write(w))?;

        //TODO
        //nal_hrd_parameters_present_flag
        writer.write_bit(false)?;

        //vcl_hrd_parameters_present_flag
        writer.write_bit(false)?;

        //if( nal_hrd_parameters_present_flag || vcl_hrd_parameters_present_flag ) low_delay_hrd_flag

        writer.write_bit(self.pic_struct_present_flag)?;

        write_optional(writer, &self.bitstream_restriction, |w, v| v.write(w))?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitstream_io::{BigEndian, BitReader, BitWriter};

    #[test]
    fn test_sps_reencode() {
        let rbsp: &[u8] = &[
            100, 0, 40, 172, 180, 3, 192, 17, 63, 44, 32, 0, 0, 0, 32, 0, 0, 6, 1, 227, 6, 84,
        ];
        let sps = Sps::read(&mut BitReader::endian(rbsp, BigEndian)).unwrap();
        let mut rbsp_reencode = Vec::new();
        sps.write(&mut BitWriter::endian(&mut rbsp_reencode, BigEndian))
            .unwrap();
        assert_eq!(rbsp, rbsp_reencode);
    }
}
