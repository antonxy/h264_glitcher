use enum_primitive::*;

enum_from_primitive! {
    #[derive(Debug, Copy, Clone, PartialEq)]
    #[repr(u8)]
    pub enum NALUnitType {
        Unspecified = 0,
        CodedSliceNonIdr          = 1,
        CodedSliceDataPartitionA = 2,
        CodedSliceDataPartitionB = 3,
        CodedSliceDataPartitionC = 4,
        CodedSliceIdr              = 5,
        Sei                          = 6,
        Sps                          = 7,
        Pps                          = 8,
        Aud                          = 9,
        EndOfSequence              = 10,
        EndOfStream                = 11,
        Filler                       = 12,
        SpsExt                      = 13,
        PrefixNal                   = 14,
        SubsetSps                   = 15,
        Dps                          = 16,
        CodedSliceAux              = 19,
        CodedSliceSvcExtension    = 20,
        NAL21 = 21,
    }
}

impl NALUnitType {
    pub fn is_picture_data(&self) -> bool {
        match self {
            NALUnitType::CodedSliceIdr | NALUnitType::CodedSliceNonIdr => { true },
            _ => { false },
        }
    }

    pub fn idr_pic_flag(&self) -> bool {
        match self {
            NALUnitType::CodedSliceIdr => true,
            _ => false,
        }
    }
}
