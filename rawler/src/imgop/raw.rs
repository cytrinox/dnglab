// SPDX-License-Identifier: LGPL-2.1
// Copyright 2021 Daniel Vogelbacher <daniel@chaospixel.com>

use super::gamma::apply_gamma;
use super::sensor::bayer::ppg::demosaic_ppg;
use super::sensor::bayer::BayerPattern;
use super::xyz::Illuminant;
use super::{Dim2, Point, Result};
use crate::imgop::matrix::{multiply, normalize, pseudo_inverse};
use crate::imgop::xyz::SRGB_TO_XYZ_D65;
use crate::imgop::Rect;
use crate::pixarray::RgbF32;
use crate::CFA;
use std::iter;

use multiversion::multiversion;
use rayon::prelude::*;

/// Conversion matrix for a specific illuminant
#[derive(Debug)]
pub struct ColorMatrix {
  pub illuminant: Illuminant,
  pub matrix: [[f32; 3]; 4],
}

/// Parameters for raw image development
#[derive(Debug)]
pub struct DevelopParams {
  pub width: usize,
  pub height: usize,
  pub color_matrices: Vec<ColorMatrix>,
  pub white_level: Vec<u16>,
  pub black_level: Vec<u16>,
  pub pattern: BayerPattern,
  pub cfa: CFA,
  pub wb_coeff: [f32; 4],
  pub active_area: Option<Rect>,
  pub crop_area: Option<Rect>,
  pub gamma: f32,
}

/// CLip only underflow values < 0.0
pub fn clip_negative<const N: usize>(pix: &[f32; N]) -> [f32; N] {
  pix.map(|p| if p < 0.0 { 0.0 } else { p })
}

/// Clip only overflow values > 1.0
pub fn clip_overflow<const N: usize>(pix: &[f32; N]) -> [f32; N] {
  pix.map(|p| if p > 1.0 { 1.0 } else { p })
}

/// Clip into range of 0.0 - 1.0
pub fn clip<const N: usize>(pix: &[f32; N]) -> [f32; N] {
  clip_range(pix, 0.0, 1.0)
}

/// Clip into range of lower and upper bound
pub fn clip_range<const N: usize>(pix: &[f32; N], lower: f32, upper: f32) -> [f32; N] {
  pix.map(|p| {
    if p < lower {
      lower
    } else if p > upper {
      upper
    } else {
      p
    }
  })
}

/// Clip pixel with N components by:
/// 1. Normalize pixel by max(pix) if any component is > 1.0
/// 2. Compute euclidean norm of the pixel, normalized by sqrt(N)
/// 3. Compute channel-wise average of normalized pixel + euclidean norm
pub fn clip_euclidean_norm_avg<const N: usize>(pix: &[f32; N]) -> [f32; N] {
  let pix = clip_negative(pix);
  let max_val = pix.iter().copied().reduce(f32::max).unwrap_or(f32::NAN);
  if max_val > 1.0 {
    // Retains color
    let color = pix.map(|p| p / max_val);
    // Euclidean norm
    let eucl = pix.map(|p| p.powi(2)).iter().sum::<f32>().sqrt() / (N as f32).sqrt();
    // Take average of both
    color.map(|p| (p + eucl) / 2.0)
  } else {
    pix
  }
}

/// Correct data by blacklevel and whitelevel on CFA (bayer) data.
/// This version is optimized vor vectorization, so please check
/// modifications on godbolt before committing.
#[multiversion]
#[clone(target = "[x86|x86_64]+avx+avx2")]
#[clone(target = "x86+sse")]
fn correct_blacklevel(raw: &mut [f32], width: usize, height: usize, blacklevel: &[f32; 4], whitelevel: &[f32; 4]) {
  assert_eq!(width % 2, 0);
  assert_eq!(height % 2, 0);
  // max value can be pre-computed for all channels.
  let max = [
    whitelevel[0] - blacklevel[0],
    whitelevel[1] - blacklevel[1],
    whitelevel[2] - blacklevel[2],
    whitelevel[3] - blacklevel[3],
  ];
  let clip = |v: f32| {
    if v.is_sign_negative() {
      0.0
    } else {
      v
    }
  };
  // Process two bayer lines at once.
  raw.par_chunks_exact_mut(width * 2).for_each(|lines| {
    // It's bayer data, so we have two lines for sure.
    let (line0, line1) = lines.split_at_mut(width);
    //line0.array_chunks_mut::<2>().zip(line1.array_chunks_mut::<2>()).for_each(|(a, b)| {
    line0.chunks_exact_mut(2).zip(line1.chunks_exact_mut(2)).for_each(|(a, b)| {
      a[0] = clip(a[0] - blacklevel[0]) / max[0];
      a[1] = clip(a[1] - blacklevel[1]) / max[1];
      b[0] = clip(b[0] - blacklevel[2]) / max[2];
      b[1] = clip(b[1] - blacklevel[3]) / max[3];
    });
  });
}

#[multiversion]
#[clone(target = "[x86|x86_64]+avx+avx2")]
#[clone(target = "x86+sse")]
fn raw_u16_to_float(pix: &[u16]) -> Vec<f32> {
  pix.iter().copied().map(f32::from).collect()
}

/// Rescale raw pixels by removing black level and scale to white level
/// Clip negative values to zero.
/// TODO: remove this old code, only used by superbayer
pub fn rescale<const N: usize>(pix: &[u16; N], black_level: &[f32; N], white_level: &[f32; N]) -> [f32; N] {
  let mut out = [f32::default(); N];
  for i in 0..N {
    out[i] = (pix[i] as f32 - black_level[i]) / (white_level[i] - black_level[i]);
  }
  clip_negative(&out)
}

#[multiversion]
#[clone(target = "[x86|x86_64]+avx+avx2")]
#[clone(target = "x86+sse")]
fn rgb_to_srgb_with_wb(rgb: &mut RgbF32, wb_coeff: &[f32; 4], xyz2cam: [[f32; 3]; 4], gamma: f32) {
  let rgb2cam = normalize(multiply(&xyz2cam, &SRGB_TO_XYZ_D65));
  let cam2rgb = pseudo_inverse(rgb2cam);

  rgb.for_each(|pix| {
    // We apply wb coeffs on the fly
    let r = pix[0] * wb_coeff[0];
    let g = pix[1] * wb_coeff[1];
    let b = pix[2] * wb_coeff[2];
    let srgb = [
      cam2rgb[0][0] * r + cam2rgb[0][1] * g + cam2rgb[0][2] * b,
      cam2rgb[1][0] * r + cam2rgb[1][1] * g + cam2rgb[1][2] * b,
      cam2rgb[2][0] * r + cam2rgb[2][1] * g + cam2rgb[2][2] * b,
    ];
    let mut clippd = clip_euclidean_norm_avg(&srgb);
    clippd.iter_mut().for_each(|p| *p = apply_gamma(*p, gamma));
    clippd
  });
}

/// Develop a RAW image to sRGB
pub fn develop_raw_srgb(pixels: &[u16], params: &DevelopParams) -> Result<(Vec<f32>, Dim2)> {
  let black_level: [f32; 4] = match params.black_level.len() {
    1 => Ok(collect_array(iter::repeat(params.black_level[0] as f32))),
    4 => Ok(collect_array(params.black_level.iter().map(|p| *p as f32))),
    c => Err(format!("Black level sample count of {} is invalid", c)),
  }?;
  let white_level: [f32; 4] = match params.white_level.len() {
    1 => Ok(collect_array(iter::repeat(params.white_level[0] as f32))),
    4 => Ok(collect_array(params.white_level.iter().map(|p| *p as f32))),
    c => Err(format!("White level sample count of {} is invalid", c)),
  }?;
  let wb_coeff: [f32; 4] = params.wb_coeff;

  log::debug!("Develop raw, wb: {:?}, black: {:?}, white: {:?}", wb_coeff, black_level, white_level);

  //Color Space Conversion
  let xyz2cam = params
    .color_matrices
    .iter()
    .find(|m| m.illuminant == Illuminant::D65)
    .ok_or("Illuminant matrix D65 not found")?
    .matrix;

  let active_area = params
    .active_area
    .unwrap_or_else(|| Rect::new_with_points(Point::zero(), Point::new(params.width, params.height)));

  let mut pixels = raw_u16_to_float(pixels);

  correct_blacklevel(&mut pixels, params.width, params.height, &black_level, &white_level);

  let mut rgb = demosaic_ppg(&pixels, Dim2::new(params.width, params.height), params.cfa.clone(), active_area);

  let w = active_area.d.w;
  let h = active_area.d.h;

  //eprintln!("cam2rgb: {:?}", cam2rgb);
  //let cropped_pixels = crop(pixels, Dim2::new(params.width, params.height), active_area);

  //let (rgb, w, h) = debayer_superpixel(&cropped_pixels, params.pattern, active_area.d, &black_level, &white_level, &wb_coeff);

  // Convert to sRGB from XYZ

  rgb_to_srgb_with_wb(&mut rgb, &wb_coeff, xyz2cam, params.gamma);

  // Flatten into Vec<f32>
  let srgb: Vec<f32> = rgb.into_inner().into_iter().flatten().collect();

  Ok((srgb, Dim2::new(w, h)))
}

/// Collect iterator into array
fn collect_array<T, I, const N: usize>(itr: I) -> [T; N]
where
  T: Default + Copy,
  I: IntoIterator<Item = T>,
{
  let mut res = [T::default(); N];
  for (it, elem) in res.iter_mut().zip(itr) {
    *it = elem
  }

  res
}

/// Calculate the multiplicative invert of an array
/// (sum of each row equals to 1.0)
pub fn mul_invert_array<const N: usize>(a: &[f32; N]) -> [f32; N] {
  let mut b = [f32::default(); N];
  b.iter_mut().zip(a.iter()).for_each(|(x, y)| *x = 1.0 / y);
  b
}

pub fn rotate_90(src: &[u16], dst: &mut [u16], width: usize, height: usize) {
  let dst = &mut dst[..src.len()]; // optimize len hints for compiler
  let owidth = height;
  for (row, line) in src.chunks_exact(width).enumerate() {
    for (col, pix) in line.iter().enumerate() {
      let orow = col;
      let ocol = (owidth - 1) - row; // inverse
      dst[orow * owidth + ocol] = *pix;
    }
  }
}
