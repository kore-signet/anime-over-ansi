#![cfg_attr(
    target_os = "cuda",
    no_std,
    feature(register_attr),
    register_attr(nvvm_internal)
)]
#![allow(improper_ctypes_definitions)]

mod palette;
use palette::*;

extern crate alloc;

use cuda_std::*;
use kasi_kule::*;

const BAYER_8X8: [usize; 64] = [
    0, 48, 12, 60, 3, 51, 15, 63, 32, 16, 44, 28, 35, 19, 47, 31, 8, 56, 4, 52, 11, 59, 7, 55, 40,
    24, 36, 20, 43, 27, 39, 23, 2, 50, 14, 62, 1, 49, 13, 61, 34, 18, 46, 30, 33, 17, 45, 29, 10,
    58, 6, 54, 9, 57, 5, 53, 42, 26, 38, 22, 41, 25, 37, 21,
];
const BAYER_4X4: [usize; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
const BAYER_2X2: [usize; 4] = [0, 2, 3, 1];

#[inline]
unsafe fn smallest_dist(c: &Jab<UCS>) -> usize {
    let mut min_dist: f32 = f32::MAX;
    let mut idx = 0;

    for i in 0..256 {
        let dist = PALETTE_JAB[i].squared_difference(c);
        if dist < min_dist {
            min_dist = dist;
            idx = i;
        }
    }

    idx
}

#[inline]
fn to_luma(c: [u8; 3]) -> f32 {
    (c[0] as f32 * 299.0 + c[1] as f32 * 587.0 + c[2] as f32 * 114.0) / 255000.0
}

macro_rules! dither_def {
    ($name:ident, $total_size: literal, $matrix_size:literal, $matrix:ident) => {
        #[kernel]
        pub unsafe fn $name(colors: &[u8], c: *mut u8, width: u32, mult: f32) {
            let idx = thread::index_1d();
            let idx_rgb = (idx * 3) as usize;
            if idx_rgb + 2 >= colors.len() {
                return;
            }

            let (x, y) = (idx % width, idx / width);

            let mut err_acc: [u8; 3] = [0, 0, 0];

            let color = [colors[idx_rgb], colors[idx_rgb + 1], colors[idx_rgb + 2]];

            let mut candidates: [([u8; 3], u8); $total_size] = [([0, 0, 0], 0); $total_size];

            for j in 0..$total_size {
                let tmp = [
                    (color[0] as f32 + (err_acc[0] as f32 * mult)).clamp(0.0, 255.0) as u8,
                    (color[1] as f32 + (err_acc[1] as f32 * mult)).clamp(0.0, 255.0) as u8,
                    (color[2] as f32 + (err_acc[2] as f32 * mult)).clamp(0.0, 255.0) as u8,
                ];

                let tmp_jab = Jab::<UCS>::from(tmp);
                let chosen_idx: usize = smallest_dist(&tmp_jab);
                let chosen = PALETTE.get_unchecked(chosen_idx);

                candidates[j] = (*chosen, chosen_idx as u8);

                err_acc[0] = err_acc[0].saturating_add(color[0].saturating_sub(chosen[0]));
                err_acc[1] = err_acc[1].saturating_add(color[1].saturating_sub(chosen[1]));
                err_acc[2] = err_acc[2].saturating_add(color[2].saturating_sub(chosen[2]));
            }

            candidates.sort_by(|a, b| to_luma(a.0).partial_cmp(&to_luma(b.0)).unwrap());
            let (_, chosen_idx) = candidates
                [$matrix[(y as usize % $matrix_size) * $matrix_size + (x as usize % $matrix_size)]];

            let elem = &mut *c.add(idx as usize);
            *elem = chosen_idx as u8;
        }
    };
}

dither_def!(dither_2x2, 4, 2, BAYER_2X2);

dither_def!(dither_4x4, 16, 4, BAYER_4X4);

dither_def!(dither_8x8, 64, 8, BAYER_8X8);
