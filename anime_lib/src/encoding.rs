use super::{
    metadata::{ColorMode, CompressionMode},
    palette::{LABAnsiColorMap, REVERSE_PALETTE},
};
use fast_image_resize as fr;
use image::{
    buffer::ConvertBuffer,
    imageops::{self},
    Rgb, RgbImage, RgbaImage,
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
            time,
            duration,
            checksum: adler32(&data.as_slice()),
            length: data.len() as u64,
            encoder_opts,
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

pub trait AnsiEncoder {
    fn color(&self, pixel: &Rgb<u8>, fg: bool) -> String {
        match self.needs_color() {
            ColorMode::EightBit => {
                if fg {
                    format!(
                        "\x1B[38;5;{}m",
                        REVERSE_PALETTE[&(pixel[0], pixel[1], pixel[2])]
                    )
                } else {
                    format!(
                        "\x1B[48;5;{}m",
                        REVERSE_PALETTE[&(pixel[0], pixel[1], pixel[2])]
                    )
                }
            }
            _ => {
                if fg {
                    format!(
                        "\x1B[38;2;{r};{g};{b}m",
                        r = pixel[0],
                        g = pixel[1],
                        b = pixel[2]
                    )
                } else {
                    format!(
                        "\x1B[48;2;{r};{g};{b}m",
                        r = pixel[0],
                        g = pixel[1],
                        b = pixel[2]
                    )
                }
            }
        }
    }

    fn encode_frame(&self, image: &RgbImage) -> String {
        let mut last_upper: Option<Rgb<u8>> = None;
        let mut last_lower: Option<Rgb<u8>> = None;

        let mut frame = String::with_capacity((image.width() * image.height()) as usize);
        for y in (0..image.height() - 1).step_by(2) {
            for x in 0..image.width() {
                let upper = image.get_pixel(x, y);
                let lower = image.get_pixel(x, y + 1);

                if last_upper.is_none() || &last_upper.unwrap() != upper {
                    frame += &self.color(upper, true);
                }

                if last_lower.is_none() || &last_lower.unwrap() != lower {
                    frame += &self.color(lower, false);
                }

                frame += "▀";

                last_upper = Some(*upper);
                last_lower = Some(*lower);
            }
            frame += "\n";
        }

        frame
    }

    fn needs_width(&self) -> u32;
    fn needs_height(&self) -> u32;
    fn needs_color(&self) -> ColorMode;
}

pub trait PacketTransformer {
    type Source;
    fn encode_packet(&self, src: &Self::Source) -> Option<EncodedPacket>;
}

pub trait PacketDecoder {
    type Output;
    fn decode_packet(&mut self, src: EncodedPacket) -> Option<Self::Output>;
}
