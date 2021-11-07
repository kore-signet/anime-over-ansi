use crate::palette::*;
use argmm::ArgMinMax;
use lazy_static::lazy_static;

// D65 standard illuminant refs
static REF_X: f64 = 95.047;
static REF_Y: f64 = 100.000;
static REF_Z: f64 = 108.883;
use lab::Lab;

lazy_static! {
    static ref LAB_PALETTE: Vec<(u8, f32, f32, f32)> = {
        PALETTE
            .iter()
            .enumerate()
            .map(|(i, (r, g, b))| {
                let lab = Lab::from_rgb(&[*r, *g, *b]);
                (i as u8, lab.l, lab.a, lab.b)
            })
            .collect()
    };
}

pub fn closest_ansi(r: u8, g: u8, b: u8) -> u8 {
    let lab = Lab::from_rgb(&[r, g, b]);

    LAB_PALETTE
        .iter()
        .map(|(_idx, p_l, p_a, p_b)| {
            (lab.l - p_l).powi(2) + (lab.a - p_a).powi(2) + (lab.b - p_b).powi(2)
        })
        .collect::<Vec<f32>>()
        .argmin()
        .unwrap() as u8
}

pub fn rgb_to_xyz(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let r = {
        let r_ = r / 255.0;
        if r_ > 0.04045 {
            ((r_ + 0.055) / 1.055).powf(2.4)
        } else {
            r_ / 12.92
        }
    } * 100.0;

    let g = {
        let g_ = g / 255.0;
        if g_ > 0.04045 {
            ((g_ + 0.055) / 1.055).powf(2.4)
        } else {
            g_ / 12.92
        }
    } * 100.0;

    let b = {
        let b_ = b / 255.0;
        if b_ > 0.04045 {
            ((b_ + 0.055) / 1.055).powf(2.4)
        } else {
            b_ / 12.92
        }
    } * 100.0;

    (
        r * 0.4124 + g * 0.3576 + b * 0.1805, // x
        r * 0.2166 + g * 0.7152 + b * 0.0722, // y
        r * 0.0193 + g * 0.1192 + b * 0.9505, // z
    )
}

pub fn xyz_to_lab(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let x = {
        let x_ = x / REF_X;
        if x_ > 0.008856 {
            x_.powf(1.0 / 3.0)
        } else {
            (7.787 * x_) + 16.0 / 116.0
        }
    };

    let y = {
        let y_ = y / REF_Y;
        if y_ > 0.008856 {
            y_.powf(1.0 / 3.0)
        } else {
            (7.787 * y_) + 16.0 / 116.0
        }
    };

    let z = {
        let z_ = z / REF_Z;
        if z_ > 0.008856 {
            z_.powf(1.0 / 3.0)
        } else {
            (7.787 * z_) + 16.0 / 116.0
        }
    };

    (
        (116.0 * y) - 16.0, // l
        500.0 * (x - y),    // a
        200.0 * (y - z),    // b
    )
}
