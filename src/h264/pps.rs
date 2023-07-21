use crate::h264::{
    more_rbsp_data, read_optional_unimplemented, read_rbsp_trailing_bits, read_se, read_ue,
    ParseError,
};
use bitstream_io::BitRead;
use visit_diff::Diff;

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct Pps {
    pub pic_parameter_set_id: u32,
    pub seq_parameter_set_id: u32,
    pub entropy_coding_mode_flag: bool,
    pub bottom_field_pic_order_in_frame_present_flag: bool,
    //pub slice_groups: Vec<SliceGroup>,
    pub num_ref_idx_l0_default_active_minus1: u8, // 0 to 31
    pub num_ref_idx_l1_default_active_minus1: u8, // 0 to 31
    pub weighted_pred_flag: bool,
    pub weighted_bipred_idc: u8,
    pub pic_init_qp_minus26: i8,
    pub pic_init_qs_minus26: i8,
    pub chroma_qp_index_offset: i8,
    pub deblocking_filter_control_present_flag: bool,
    pub constrained_intra_pred_flag: bool,
    pub redundant_pic_cnt_present_flag: bool,
    pub pps_more_data: Option<PpsMoreData>,
}

impl Pps {
    pub fn read<R: BitRead + Clone>(reader: &mut R) -> Result<Self, ParseError> {
        let pic_parameter_set_id = read_ue(reader)?;
        let seq_parameter_set_id = read_ue(reader)?;
        let entropy_coding_mode_flag = reader.read_bit()?;
        let bottom_field_pic_order_in_frame_present_flag = reader.read_bit()?;

        //TODO slice_groups
        if read_ue::<u32, _>(reader)? != 0 {
            return Err(ParseError::Unimplemented);
        }

        let num_ref_idx_l0_default_active_minus1 = read_ue(reader)?;
        let num_ref_idx_l1_default_active_minus1 = read_ue(reader)?;
        let weighted_pred_flag = reader.read_bit()?;
        let weighted_bipred_idc = reader.read(2)?;
        let pic_init_qp_minus26 = read_se(reader)?;
        let pic_init_qs_minus26 = read_se(reader)?;
        let chroma_qp_index_offset = read_se(reader)?;
        let deblocking_filter_control_present_flag = reader.read_bit()?;
        let constrained_intra_pred_flag = reader.read_bit()?;
        let redundant_pic_cnt_present_flag = reader.read_bit()?;

        let mut pps_more_data = None;
        if more_rbsp_data(reader)? {
            pps_more_data = Some(PpsMoreData::read(reader)?);
        }

        read_rbsp_trailing_bits(reader)?;

        Ok(Self {
            pic_parameter_set_id,
            seq_parameter_set_id,
            entropy_coding_mode_flag,
            bottom_field_pic_order_in_frame_present_flag,
            num_ref_idx_l0_default_active_minus1,
            num_ref_idx_l1_default_active_minus1,
            weighted_pred_flag,
            weighted_bipred_idc,
            pic_init_qp_minus26,
            pic_init_qs_minus26,
            chroma_qp_index_offset,
            deblocking_filter_control_present_flag,
            constrained_intra_pred_flag,
            redundant_pic_cnt_present_flag,
            pps_more_data,
        })
    }
}

#[derive(Clone, Debug, Diff, PartialEq)]
pub struct PpsMoreData {
    pub transform_8x8_mode_flag: bool,
    // pic_scaling_matrix_present_flag,
    pub second_chroma_qp_index_offset: i8,
}

impl PpsMoreData {
    pub fn read(reader: &mut impl BitRead) -> Result<Self, ParseError> {
        let transform_8x8_mode_flag = reader.read_bit()?;
        read_optional_unimplemented(reader)?; //pic_scaling_matrix_present_flag
        let second_chroma_qp_index_offset = read_se(reader)?;
        Ok(Self {
            transform_8x8_mode_flag,
            second_chroma_qp_index_offset,
        })
    }
}
