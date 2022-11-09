use crate::palette::*;
use argmm::ArgMinMax;
use kasi_kule::{Jab, UCS};

/// Get closest ansi256 color using DeltaE distance. Accelerated with AVX instructions.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx")]
pub unsafe fn closest_ansi_avx(rgb: &[u8; 3]) -> (u8, f32) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let jab = Jab::<UCS>::from(*rgb);
    let lab_arr = [jab.J, jab.a, jab.b, 0.0, jab.J, jab.a, jab.b, 0.0];
    let lab_mm = _mm256_loadu_ps(lab_arr.as_ptr() as *const f32);

    let mut res_array: [f32; 256] = [0.0; 256]; // full delta E
    let mut tmp: [f32; 8] = [0.0; 8]; // tmp array for storing intermediate values

    JAB_PALETTE_FLATTENED
        .chunks_exact(16)
        .enumerate()
        .for_each(|(i, step)| {
            let pal_a = _mm256_loadu_ps(step.as_ptr() as *const f32); // load in 8 values (l,a,b,0,l,a,b,0)
            let mut a = _mm256_sub_ps(lab_mm, pal_a); // subtract (lhs.l - rhs.l), (lhs.a - rhs.a), (lhs.b - rhs.b)
            a = _mm256_mul_ps(a, a); // raise to power of two

            let pal_b = _mm256_loadu_ps(step.as_ptr().add(8) as *const f32); // load in 8 values (l,a,b,0,l,a,b,0)
            let mut b = _mm256_sub_ps(lab_mm, pal_b); // subtract (lhs.l - rhs.l), (lhs.a - rhs.a), (lhs.b - rhs.b)
            b = _mm256_mul_ps(b, b); // raise to power of two

            _mm256_storeu_ps(tmp.as_mut_ptr() as *mut f32, _mm256_hadd_ps(a, b)); // add up (l + a) for every value and then store
            let start = i * 4;
            // add up (l + a) + b
            *res_array.get_unchecked_mut(start) = tmp.get_unchecked(0) + tmp.get_unchecked(1);
            *res_array.get_unchecked_mut(start + 1) = tmp.get_unchecked(4) + tmp.get_unchecked(5);
            *res_array.get_unchecked_mut(start + 2) = tmp.get_unchecked(2) + tmp.get_unchecked(3);
            *res_array.get_unchecked_mut(start + 3) = tmp.get_unchecked(6) + tmp.get_unchecked(7);
        });

    let v = res_array.argmin().unwrap();
    (v as u8, *res_array.get_unchecked(v))
}

/// Get closest ansi256 color using DeltaE distance. Accelerated with SSE instructions.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse")]
pub unsafe fn closest_ansi_sse(rgb: &[u8; 3]) -> (u8, f32) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let jab = Jab::<UCS>::from(*rgb);
    let lab_arr = [jab.J, jab.a, jab.b, 0.0];
    let lab_mm = _mm_loadu_ps(lab_arr.as_ptr() as *const f32);

    let mut results: [f32; 256] = [0.0; 256];
    let mut res_array: [f32; 4] = [0.0; 4];

    JAB_PALETTE_FLATTENED
        .chunks_exact(4)
        .enumerate()
        .for_each(|(i, step)| {
            let pal_mm = _mm_loadu_ps(step.as_ptr() as *const f32);
            let mut res = _mm_sub_ps(lab_mm, pal_mm);
            res = _mm_mul_ps(res, res);
            _mm_store_ps(res_array.as_mut_ptr() as *mut f32, res); // store back

            *results.get_unchecked_mut(i) = res_array.get_unchecked(0) // add up left delta E
                + res_array.get_unchecked(1)
                + res_array.get_unchecked(2);
        });

    let v = res_array.argmin().unwrap();
    (v as u8, *res_array.get_unchecked(v))
}

/// Get closest ansi256 color using Jab/CAM02 distance. No acceleration.
pub fn closest_ansi_scalar(rgb: &[u8; 3]) -> (u8, f32) {
    let jab = Jab::<UCS>::from(*rgb);
    let mut results: [f32; 256] = [0.0; 256];
    for i in 0..256 {
        let (p_j, p_a, p_b) = JAB_PALETTE[i];
        results[i] = (jab.J - p_j).powi(2) + (jab.a - p_a).powi(2) + (jab.b - p_b).powi(2);
    }

    let v = results.argmin().unwrap();
    (v as u8, results[v])
}

/// Get closest ansi256 color using Jab/CAM02 distance. Accelerated with SIMD intrinsics if available.
pub fn closest_ansi(rgb: &[u8; 3]) -> (u8, f32) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx") {
            return unsafe { closest_ansi_avx(rgb) };
        } else if is_x86_feature_detected!("sse") {
            return unsafe { closest_ansi_sse(rgb) };
        }
    }

    closest_ansi_scalar(rgb)
}
