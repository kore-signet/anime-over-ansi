use super::{
    metadata::{ColorMode, CompressionMode},
    palette::LABAnsiColorMap,
};
use fast_image_resize as fr;
use image::{
    buffer::ConvertBuffer,
    imageops::{self},
    RgbImage, RgbaImage,
};
use simd_adler32::adler32;
use std::collections::HashSet;

use std::num::NonZeroU32;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub struct EncoderOptions {
    pub compression_mode: CompressionMode,
    pub compression_level: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct EncodedPacket {
    pub stream_index: u32,
    pub checksum: u32,
    pub length: u64,
    pub time: Duration,
    pub duration: Option<Duration>,
    pub data: Vec<u8>,
    pub encoder_opts: Option<EncoderOptions>,
}

impl EncodedPacket {
    pub fn from_data(
        stream_index: u32,
        time: Duration,
        duration: Option<Duration>,
        data: Vec<u8>,
        encoder_opts: Option<EncoderOptions>,
    ) -> EncodedPacket {
        EncodedPacket {
            time: time,
            duration: duration,
            checksum: adler32(&data.as_slice()),
            length: data.len() as u64,
            encoder_opts: encoder_opts,
            stream_index,
            data,
        }
    }

    pub fn map_data(self, data: Vec<u8>) -> EncodedPacket {
        EncodedPacket {
            time: self.time,
            duration: self.duration,
            checksum: adler32(&data.as_slice()),
            length: data.len() as u64,
            encoder_opts: self.encoder_opts,
            stream_index: self.stream_index,
            data,
        }
    }

    pub fn switch_data(&mut self, data: Vec<u8>) {
        self.checksum = adler32(&data.as_slice());
        self.length = data.len() as u64;
        self.data = data;
    }
}

pub struct ProcessorPipeline {
    pub filter: fr::FilterType,
    pub width: u32,
    pub height: u32,
    pub color_modes: HashSet<ColorMode>,
}

impl ProcessorPipeline {
    pub fn process(&self, img: &RgbaImage) -> Vec<(ColorMode, RgbImage)> {
        let src_image = fr::Image::from_vec_u8(
            NonZeroU32::new(img.width()).unwrap(),
            NonZeroU32::new(img.height()).unwrap(),
            img.clone().into_raw(),
            fr::PixelType::U8x4,
        )
        .unwrap();

        let mut dst_image = fr::Image::new(
            NonZeroU32::new(self.width).unwrap(),
            NonZeroU32::new(self.height).unwrap(),
            src_image.pixel_type(),
        );
        let mut dst_view = dst_image.view_mut();
        let mut resizer = fr::Resizer::new(fr::ResizeAlg::Convolution(self.filter));
        resizer.resize(&src_image.view(), &mut dst_view).unwrap();

        let frame: RgbImage =
            RgbaImage::from_raw(self.width, self.height, dst_image.buffer().to_vec())
                .unwrap()
                .convert();

        let mut res = Vec::with_capacity(self.color_modes.len());

        for mode in &self.color_modes {
            if mode == &ColorMode::EightBit {
                let mut dframe = frame.clone();
                imageops::dither(&mut dframe, &LABAnsiColorMap);
                res.push((*mode, dframe));
            } else {
                res.push((*mode, frame.clone()));
            }
        }

        res
    }
}
