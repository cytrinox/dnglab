// SPDX-License-Identifier: LGPL-2.1
// Copyright 2021 Daniel Vogelbacher <daniel@chaospixel.com>

use crate::{
  cfa::PlaneColor,
  imgop::{sensor::bayer::RgbBayerPattern, Dim2, Rect},
  pixarray::Color2D,
  CFA,
};
use rayon::prelude::*;

use super::Demosaic;

#[derive(Default)]
pub struct Superpixel3Channel {}

impl Superpixel3Channel {
  pub fn new() -> Self {
    Self {}
  }
}

impl Demosaic<f32, 3> for Superpixel3Channel {
  /// Debayer image by using superpixel method.
  /// Each output pixel RGB tuple is generated by 4 pixels from input.
  /// The result image is 1/4 of size.
  fn demosaic(&self, pixels: &[f32], dim: Dim2, cfa: &CFA, colors: &PlaneColor, roi: Rect) -> Color2D<f32, 3> {
    assert!(roi.p.x == 0 && roi.p.y == 0 && dim == roi.d, "ROI demosaic is not yet supported for Superpixel"); // TODO: implement me
    if colors.plane_count() != 3 {
      panic!("Demosaic for 3 channels needs 3 color planes, but {} given", colors.plane_count());
    }
    if !cfa.is_rgb() {
      panic!("Demosaic for 3 channels requires RGB CFA pattern, but CFA {} given", cfa);
    }
    let cfa = cfa.shift(roi.p.x, roi.p.y);
    let pattern = match cfa.name.as_str() {
      "RGGB" => RgbBayerPattern::RGGB,
      "BGGR" => RgbBayerPattern::BGGR,
      "GBRG" => RgbBayerPattern::GBRG,
      "GRBG" => RgbBayerPattern::GRBG,
      _ => unreachable!(), // Guarded by is_rgb()
    };
    let rgb = pixels
      .par_chunks_exact(dim.w * 2)
      .map(|s| {
        let (r1, r2) = s.split_at(dim.w);
        r1.chunks_exact(2)
          .zip(r2.chunks_exact(2))
          .map(|(a, b)| {
            let p = [a[0], a[1], b[0], b[1]];
            match pattern {
              RgbBayerPattern::RGGB => [p[0], (p[1] + p[2]) / 2.0, p[3]],
              RgbBayerPattern::BGGR => [p[3], (p[1] + p[2]) / 2.0, p[0]],
              RgbBayerPattern::GBRG => [p[2], (p[0] + p[3]) / 2.0, p[1]],
              RgbBayerPattern::GRBG => [p[1], (p[0] + p[3]) / 2.0, p[2]],
            }
          })
          .collect::<Vec<_>>()
      })
      .flatten()
      .collect();
    Color2D::new_with(rgb, dim.w >> 1, dim.h >> 1)
  }
}

#[derive(Default)]
pub struct Superpixel4Channel {}

impl Superpixel4Channel {
  pub fn new() -> Self {
    Self {}
  }
}

impl Demosaic<f32, 4> for Superpixel4Channel {
  /// Debayer image by using superpixel method.
  /// Each output pixel RGB tuple is generated by 4 pixels from input.
  /// The result image is 1/4 of size.
  fn demosaic(&self, pixels: &[f32], dim: Dim2, cfa: &CFA, colors: &PlaneColor, roi: Rect) -> Color2D<f32, 4> {
    assert!(roi.p.x == 0 && roi.p.y == 0 && dim == roi.d, "ROI demosaic is not yet supported for Superpixel"); // TODO: implement me
    if colors.plane_count() != 4 {
      panic!("Demosaic for 4 channels needs 4 color planes, but {} given", colors.plane_count());
    }
    log::debug!("Superpixel debayer ROI: {:?}", roi);

    // We debayer the whole frame, not only ROI. So we must not shift CFA!
    // let cfa = cfa.shift(roi.p.x, roi.p.y);

    // Index into colormap is plane number, value is the index into 2x2 Bayer superpixel.
    let colormap: [usize; 4] = colors.plane_colors::<4>().map(|c| PlaneColor::cfa_index(cfa, c));

    let out = pixels
      .par_chunks_exact(dim.w * 2)
      .map(|s| {
        let (r1, r2) = s.split_at(dim.w);
        r1.chunks_exact(2)
          .zip(r2.chunks_exact(2))
          .map(|(a, b)| {
            let superpixel = [a[0], a[1], b[0], b[1]];
            // Map superpixel into correct ordering (CFAPlaneColor)
            [
              superpixel[colormap[0]],
              superpixel[colormap[1]],
              superpixel[colormap[2]],
              superpixel[colormap[3]],
            ]
          })
          .collect::<Vec<_>>()
      })
      .flatten()
      .collect();
    Color2D::new_with(out, dim.w >> 1, dim.h >> 1)
  }
}
