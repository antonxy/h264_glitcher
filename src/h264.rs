use std::os::raw::c_int;

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
