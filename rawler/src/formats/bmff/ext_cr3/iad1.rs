// SPDX-License-Identifier: MIT
// Copyright 2021 Daniel Vogelbacher <daniel@chaospixel.com>

use super::super::{BoxHeader, FourCC, ReadBox, Result, read_box_header_ext};
use byteorder::{BigEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Iad1Box {
  pub header: BoxHeader,
  pub version: u8,
  pub flags: u32,
  pub img_width: u16,
  pub img_height: u16,
  pub unknown1: u16,
  pub image_type: u16,
  pub unknown2: u16,
  pub unknown3: u16,
  pub iad1_type: Iad1Type,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Iad1Type {
  Small(Iad1Small),
  Big(Iad1Big),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Iad1Small {
  pub crop_left_offset: u16,
  pub crop_top_offset: u16,
  pub crop_right_offset: u16,
  pub crop_bottom_offset: u16,
  pub sensor_x_start: u16,
  pub sensor_y_start: u16,
  pub sensor_x_end: u16,
  pub sensor_y_end: u16,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Iad1Big {
  pub crop_left_offset: u16,
  pub crop_top_offset: u16,
  pub crop_right_offset: u16,
  pub crop_bottom_offset: u16,
  pub lob_left_offset: u16,
  pub lob_top_offset: u16,
  pub lob_right_offset: u16,
  pub lob_bottom_offset: u16,
  pub tob_left_offset: u16,
  pub tob_top_offset: u16,
  pub tob_right_offset: u16,
  pub tob_bottom_offset: u16,
  pub active_area_left_offset: u16,
  pub active_area_top_offset: u16,
  pub active_area_right_offset: u16,
  pub active_area_bottom_offset: u16,
}

impl Iad1Box {
  pub const TYP: FourCC = FourCC::with(['I', 'A', 'D', '1']);
}

impl<R: Read + Seek> ReadBox<&mut R> for Iad1Box {
  fn read_box(reader: &mut R, header: BoxHeader) -> Result<Self> {
    let (version, flags) = read_box_header_ext(reader)?;

    let img_width = reader.read_u16::<BigEndian>()?;
    let img_height = reader.read_u16::<BigEndian>()?;
    let unknown1 = reader.read_u16::<BigEndian>()?;
    let image_type = reader.read_u16::<BigEndian>()?;
    let unknown2 = reader.read_u16::<BigEndian>()?;
    let unknown3 = reader.read_u16::<BigEndian>()?;

    let iad1_type = match image_type {
      0 => Iad1Type::Small(Iad1Small {
        crop_left_offset: reader.read_u16::<BigEndian>()?,
        crop_top_offset: reader.read_u16::<BigEndian>()?,
        crop_right_offset: reader.read_u16::<BigEndian>()?,
        crop_bottom_offset: reader.read_u16::<BigEndian>()?,
        sensor_x_start: reader.read_u16::<BigEndian>()?,
        sensor_y_start: reader.read_u16::<BigEndian>()?,
        sensor_x_end: reader.read_u16::<BigEndian>()?,
        sensor_y_end: reader.read_u16::<BigEndian>()?,
      }),
      2 => Iad1Type::Big(Iad1Big {
        crop_left_offset: reader.read_u16::<BigEndian>()?,
        crop_top_offset: reader.read_u16::<BigEndian>()?,
        crop_right_offset: reader.read_u16::<BigEndian>()?,
        crop_bottom_offset: reader.read_u16::<BigEndian>()?,
        lob_left_offset: reader.read_u16::<BigEndian>()?,
        lob_top_offset: reader.read_u16::<BigEndian>()?,
        lob_right_offset: reader.read_u16::<BigEndian>()?,
        lob_bottom_offset: reader.read_u16::<BigEndian>()?,
        tob_left_offset: reader.read_u16::<BigEndian>()?,
        tob_top_offset: reader.read_u16::<BigEndian>()?,
        tob_right_offset: reader.read_u16::<BigEndian>()?,
        tob_bottom_offset: reader.read_u16::<BigEndian>()?,
        active_area_left_offset: reader.read_u16::<BigEndian>()?,
        active_area_top_offset: reader.read_u16::<BigEndian>()?,
        active_area_right_offset: reader.read_u16::<BigEndian>()?,
        active_area_bottom_offset: reader.read_u16::<BigEndian>()?,
      }),
      _ => {
        panic!("Invalid iad1 type"); // TODO
      }
    };

    reader.seek(SeekFrom::Start(header.end_offset()))?;

    Ok(Self {
      header,
      version,
      flags,
      img_width,
      img_height,
      unknown1,
      image_type,
      unknown2,
      unknown3,
      iad1_type,
    })
  }
}
