use crate::h264::{NALUnitType, NalLocation, ParseError};
use std::{convert::TryInto, path::Path};
use enum_primitive::FromPrimitive;
use ffmpeg_next as ffmpeg;

pub struct AvccConfig {
    pub nal_length_size: usize,
    pub sps_nals: Vec<Vec<u8>>,
    pub pps_nals: Vec<Vec<u8>>,
}

pub struct Mp4Video {
    pub avcc_config: AvccConfig,
    pub nal_locations: Vec<NalLocation>,
}

fn parse_avcc_extradata(extradata: &[u8]) -> Result<AvccConfig, &'static str> {
    if extradata.len() < 7 {
        return Err("Extradata too short");
    }

    // Byte 4 lower 2 bits + 1 gives the NAL length prefix size (usually 4 bytes)
    let nal_length_size = ((extradata[4] & 0x03) + 1) as usize;
    let mut offset = 5;

    let read_nals = |count, offset: &mut usize| {
        let mut nals = Vec::with_capacity(count);

        for _ in 0..count {
            if *offset + 2 > extradata.len() {
                return Err("reached end of extradata");
            }
            let nal_len = u16::from_be_bytes([extradata[*offset], extradata[*offset + 1]]) as usize;
            *offset += 2;

            if *offset + nal_len > extradata.len() {
                return Err("reached end of extradata");
            }
            nals.push(extradata[*offset..*offset + nal_len].to_vec());
            *offset += nal_len;
        }

        Ok(nals)
    };

    // Read SPS
    let num_sps = (extradata[offset] & 0x1F) as usize;
    offset += 1;
    let sps_nals = read_nals(num_sps, &mut offset)?;


    // Read PPS
    if offset >= extradata.len() {
        return Err("reached end of extradata");
    }
    let num_pps = extradata[offset] as usize;
    offset += 1;
    let pps_nals = read_nals(num_pps, &mut offset)?;


    Ok(AvccConfig {
        nal_length_size,
        sps_nals,
        pps_nals,
    })
}

struct NalIterator<'a> {
    data: &'a [u8],
    nal_length_size: usize,
    offset: usize
}

impl<'a> NalIterator<'a> {
    fn new(data: &'a [u8], nal_length_size: usize) -> Self {
        Self {
            data,
            nal_length_size,
            offset: 0,
        }
    }
}

impl<'a> Iterator for NalIterator<'a> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset + self.nal_length_size <= self.data.len() {
            let nal_len = match self.nal_length_size {
                4 => u32::from_be_bytes(self.data[self.offset..self.offset + 4].try_into().unwrap()) as usize,
                2 => u16::from_be_bytes(self.data[self.offset..self.offset + 2].try_into().unwrap()) as usize,
                1 => self.data[self.offset] as usize,
                _ => return None,
            };
            self.offset += self.nal_length_size;

            if self.offset + nal_len > self.data.len() {
                return None; // Malformed packet, just ignore it
            }

            let res = Some((self.offset, nal_len));
            self.offset += nal_len;

            return res;
        }
        None
    }
}

/// Loads the mp4 file using ffmpeg and stores the avcC data and video NAL offsets
pub fn index_mp4(path: &Path) -> Result<Mp4Video, Box<dyn std::error::Error>> {
    let mut ictx = ffmpeg::format::input(path)?;

    let stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or("No video stream")?;
    let video_index = stream.index();

    // Maybe I don't even really need to parse this
    // Could keep stream.parameters() and pass it to ffmpeg decoder create
    let extradata = unsafe {
        let ptr = (*stream.parameters().as_ptr()).extradata;
        let size = (*stream.parameters().as_ptr()).extradata_size as usize;
        if ptr.is_null() || size == 0 {
            return Err("No extradata found".into());
        }
        std::slice::from_raw_parts(ptr, size)
    };
    let config = parse_avcc_extradata(extradata)?;

    let mut nal_locations = Vec::new();
    for (stream, packet) in ictx.packets() {
        if stream.index() != video_index {
            continue;
        }
        let packet_position = packet.position();
        if let Some(data) = packet.data() {
            for (offset, len) in NalIterator::new(data, config.nal_length_size){
                let nal_unit_type = NALUnitType::from_u8(data[offset] & 0x1F).ok_or(ParseError::InvalidData)?;

                if nal_unit_type == NALUnitType::CodedSliceNonIdr || nal_unit_type == NALUnitType::CodedSliceIdr {
                    nal_locations.push(NalLocation {
                        offset: packet_position as usize + offset,
                        length: len,
                    });
                }
            }
        }
    }

    Ok(Mp4Video {
        avcc_config: config,
        nal_locations
    })
}
