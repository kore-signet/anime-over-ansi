use rustacuda::launch;

use crate::color_calc::LAB_PALETTE;
use crate::palette::PALETTE;
use image::RgbImage;
use lab::Lab;
use lazy_static::lazy_static;
use rustacuda::prelude::*;
use std::ffi::CString;
use std::time::Instant;

// note! this does not work properly with processorpipeline yet, as multithreading / async-ing means that the context will get screwed up.
// it's also a mess.

static BAYER_8X8: [i32; 64] = [
    0, 48, 12, 60, 3, 51, 15, 63, 32, 16, 44, 28, 35, 19, 47, 31, 8, 56, 4, 52, 11, 59, 7, 55, 40,
    24, 36, 20, 43, 27, 39, 23, 2, 50, 14, 62, 1, 49, 13, 61, 34, 18, 46, 30, 33, 17, 45, 29, 10,
    58, 6, 54, 9, 57, 5, 53, 42, 26, 38, 22, 41, 25, 37, 21,
];

lazy_static! {
    static ref FLAT_LAB_PALETTE: [f32; 768] = {
        let mut labs: [f32; 768] = [0.0; 768];
        for (i, v) in LAB_PALETTE.iter().enumerate() {
            let offset = i * 3;
            labs[offset] = v.0;
            labs[offset + 1] = v.1;
            labs[offset + 2] = v.2;
        }
        labs
    };
    static ref FLAT_RGB_PALETTE: [f32; 768] = {
        let mut rgbs: [f32; 768] = [0.0; 768];
        for (i, v) in PALETTE.iter().enumerate() {
            let offset = i * 3;
            rgbs[offset] = v[0] as f32;
            rgbs[offset + 1] = v[1] as f32;
            rgbs[offset + 2] = v[2] as f32;
        }
        rgbs
    };
}

pub struct CudaDither {
}

impl CudaDither {
    pub fn new() -> CudaDither {
        CudaDither {}
    }

    pub fn dither(&self, image: &RgbImage, multiplier: f32) -> RgbImage {
        let module_data = CString::new(include_str!("../cuda/pattern-dither.ptx")).unwrap();
        let module = Module::load_from_string(&module_data).unwrap();

        let height = image.height() as i32;
        let width = image.width() as i32;

        let block_dimensions = (
            (width as f64 / 16.0).ceil() as u32,
            (height as f64 / 16.0).ceil() as u32,
            1,
        );

        let image_buffer = (*image).iter().map(|v| *v as f32).collect::<Vec<f32>>();
        let mut input_buffer = DeviceBuffer::from_slice(&image_buffer).unwrap();
        let mut matrix = DeviceBuffer::from_slice(&BAYER_8X8[..]).unwrap();
        let mut lab_palette = DeviceBuffer::from_slice(&FLAT_LAB_PALETTE[..]).unwrap();
        let mut rgb_palette = DeviceBuffer::from_slice(&FLAT_RGB_PALETTE[..]).unwrap();
        let mut out_buffer =
            unsafe { DeviceBuffer::zeroed((image.width() * image.height() * 3) as usize).unwrap() };

        let stream = Stream::new(StreamFlags::NON_BLOCKING, None).unwrap();
        unsafe {
            let res = launch!(
                module.delta_e<<<(16, 16, 1), block_dimensions, 0, stream>>>(
                    lab_palette.as_device_ptr(),
                    rgb_palette.as_device_ptr(),
                    input_buffer.as_device_ptr(),
                    out_buffer.as_device_ptr(),
                    height,
                    width,
                    64i32,
                    matrix.as_device_ptr(),
                    multiplier
                )
            );

            res.unwrap();
            stream.synchronize().unwrap();
        }

        let mut out_buffer_host: Vec<f32> = vec![0.0; (width as usize * height as usize * 3)];
        out_buffer.copy_to(&mut out_buffer_host).unwrap();

        RgbImage::from_vec(
            image.width(),
            image.height(),
            out_buffer_host
                .into_iter()
                .map(|v| v.round() as u8)
                .collect::<Vec<u8>>(),
        )
        .unwrap()
    }
}

// fn main() {
//     let egg = image::open("wonder-egg-scaled.png").unwrap().into_rgb8();
//     let mut standard_egg = egg.clone();
//     anime_telnet::pattern::dither(&mut standard_egg, 8, 0.09);
//     standard_egg.save("comparison.png").unwrap();
//     let _ctx = rustacuda::quick_init().unwrap();

//     let mut img_d = DeviceBuffer::from_slice(&img).unwrap();

//     let ptx = CString::new(include_str!("../test.ptx")).unwrap();
//     let module = Module::load_from_string(&ptx).unwrap();
//     let start = Instant::now();

//         stream.synchronize().unwrap();

//         let mut out_host: [f32; 62208] = [0.0; 62208];
//         out_d.copy_to(&mut out_host).unwrap();

//         let mut vec = out_host
//             .into_iter()
//             .map(|v| v.round() as u8)
//             .collect::<Vec<u8>>();
//         let mut new_image = image::RgbImage::from_vec(192, 108, vec).unwrap();
//         print!("{}", Hm.encode_frame(&new_image).0);
//         // new_image.save("out.png").unwrap();
//     }
// }
