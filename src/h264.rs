use std::os::raw::c_int;
use enum_primitive::*;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FrameType {
    P,
    B,
    I,
    POnly,
    BOnly,
    IOnly,
    SPOnly,
    SIOnly,
}

impl FrameType {
    pub fn from_sh_slice_type(sh_slice_type: c_int) -> FrameType {
        use FrameType::*;
        match sh_slice_type {
            0 => P,
            1 => B,
            2 => I,
            5 => POnly,
            6 => BOnly,
            7 => IOnly,
            8 => SPOnly,
            9 => SIOnly,
            _ => panic!("not impl"),
        }
    }
}

enum_from_primitive! {
    #[derive(Debug, Copy, Clone, PartialEq)]
    #[repr(u8)]
    pub enum NALUnitType {
        Unpecified = 0,
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
    }
}

