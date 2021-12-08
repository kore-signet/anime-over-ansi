use crate::palette::*;
use argmm::ArgMinMax;
use lab::Lab;
use lazy_static::lazy_static;

static K_L: f32 = 1.0;
static K_ONE: f32 = 0.045;
static K_TWO: f32 = 0.015;

lazy_static! {
    // ref_l, ref_a, ref_b, ref_c,
    // s_l, s_c, s_h,0.0
    static ref CIE94_DATA: [f32; 2048] = {
        let mut pal = [0.0f32; 2048];
        for i in 0..256 {
            let offset = i * 8;

            let (ref_l, ref_a, ref_b) = LAB_PALETTE[i];
            let ref_c = (ref_a.powi(2) + ref_b.powi(2)).sqrt();
            let s_c = 1.0 + K_ONE * ref_c;
            let s_h = 1.0 + K_TWO * ref_c;

            pal[offset] = ref_l;
            pal[offset + 1] = ref_a;
            pal[offset + 2] = ref_b;
            pal[offset + 3] = ref_c;
            pal[offset + 4] = 1.0;
            pal[offset + 5] = s_c;
            pal[offset + 6] = s_h;
        }

        pal
    };
}

pub fn closest_ansi_scalar(rgb: &[u8; 3]) -> (u8, f32) {
    let pixel = Lab::from_rgb(rgb);
    let mut results: [f32; 256] = [0.0; 256];
    for i in 0..256 {
        let (ref_l, ref_a, ref_b) = LAB_PALETTE[i];

        let delta_l = ref_l - pixel.l;
        let ref_c = (ref_a.powi(2) + ref_b.powi(2)).sqrt();
        let pixel_c = (pixel.a.powi(2) + pixel.b.powi(2)).sqrt();
        let delta_c = ref_c - pixel_c;

        let delta_h = (ref_a - pixel.a).powi(2) + (ref_b - pixel.b).powi(2) - delta_c.powi(2);

        let s_l = 1.0;
        let s_c = 1.0 + K_ONE * ref_c;
        let s_h = 1.0 + K_TWO * ref_c;

        let delta_e = (delta_l / (K_L * s_l)).powi(2)
            + (delta_c / (K_L * s_c)).powi(2)
            + (delta_h / (K_L * s_h)).powi(2);

        results[i] = delta_e;
    }

    let v = results.argmin().unwrap();
    (v as u8, results[v])
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse")]
pub unsafe fn closest_ansi_sse(rgb: &[u8; 3]) -> (u8, f32) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let pixel = Lab::from_rgb(rgb);
    let pixel_c = (pixel.a.powi(2) + pixel.b.powi(2)).sqrt();

    let pixel_arr = [pixel.l, pixel.a, pixel.b, pixel_c];
    let pixel_mm = _mm_loadu_ps(pixel_arr.as_ptr() as *const f32);

    let mut res_array: [f32; 256] = [0.0; 256]; // full delta E
    let mut tmp: [f32; 4] = [0.0; 4]; // tmp array for storing intermediate values

    CIE94_DATA
        .chunks_exact(8)
        .enumerate()
        .for_each(|(i, step)| {
            let ref_lab = _mm_loadu_ps(step.as_ptr()); // ref_l, ref_a, ref_b, ref_c
            let deltas = _mm_sub_ps(ref_lab, pixel_mm); // deltaL, deltaA, deltaB, deltaC

            _mm_store_ps(tmp.as_mut_ptr(), deltas);

            let delta_l = *tmp.get_unchecked(0);
            let delta_c = *tmp.get_unchecked(3);

            let delta_h_mm = _mm_mul_ps(deltas, deltas);
            _mm_store_ps(tmp.as_mut_ptr(), delta_h_mm);
            let delta_h = tmp.get_unchecked(1) + tmp.get_unchecked(2) - tmp.get_unchecked(3);

            let delta_e_weights = _mm_loadu_ps(step.as_ptr().add(4)); // s_l, s_c, s_h, 0.0
            let mut delta_e_mm = _mm_setr_ps(delta_l, delta_c, delta_h, 0.0); // deltaL, deltaC, deltaH

            delta_e_mm = _mm_div_ps(delta_e_mm, delta_e_weights);
            delta_e_mm = _mm_mul_ps(delta_e_mm, delta_e_mm);

            _mm_store_ps(tmp.as_mut_ptr(), delta_e_mm);

            *res_array.get_unchecked_mut(i) =
                tmp.get_unchecked(0) + tmp.get_unchecked(1) + tmp.get_unchecked(2);
        });

    let v = res_array.argmin().unwrap();
    (v as u8, *res_array.get_unchecked(v))
}

/// Get closest ansi256 color using CIE94 DeltaE distance. Accelerated with SIMD intrinsics if available.
pub fn closest_ansi(rgb: &[u8; 3]) -> (u8, f32) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("sse") {
            return unsafe { closest_ansi_sse(rgb) };
        }
    }

    closest_ansi_scalar(rgb)
}
