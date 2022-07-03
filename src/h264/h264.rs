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
    }
}

