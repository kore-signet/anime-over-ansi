use super::{
    metadata::ColorMode,
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
use std::fs;
use std::io::{self, Write};
use std::num::NonZeroU32;

pub enum OutputStream<'a> {
    File(fs::File),
    CompressedFile(zstd::Encoder<'a, fs::File>),
}

impl OutputStream<'_> {
    pub fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            OutputStream::File(f) => f.write_all(bytes),
            OutputStream::CompressedFile(f) => f.write_all(bytes),
        }
    }

    pub fn finish(self) -> io::Result<fs::File> {
        match self {
            OutputStream::File(mut f) => {
                f.flush()?;
                Ok(f)
            }
            OutputStream::CompressedFile(f) => f.finish(),
        }
    }
}

pub struct Encoder<'a> {
    pub needs_width: u32,
    pub needs_height: u32,
    pub needs_color: ColorMode,
    pub frame_lengths: Vec<u64>,
    pub frame_hashes: Vec<u32>,
    pub output: OutputStream<'a>,
}

impl Encoder<'_> {
    // todo: stop using rgbimage here and use a raw vec<u8> which can be 8bit or 24bit
    pub fn encode_frame(&mut self, img: &RgbImage) -> io::Result<()> {
        let mut last_upper: Option<Rgb<u8>> = None;
        let mut last_lower: Option<Rgb<u8>> = None;

        let mut frame = String::with_capacity((img.width() * img.height()) as usize);
        for y in (0..img.height() - 1).step_by(2) {
            for x in 0..img.width() {
                let upper = img.get_pixel(x, y);
                let lower = img.get_pixel(x, y + 1);

                match self.needs_color {
                    ColorMode::EightBit => {
                        if last_upper.is_none() || &last_upper.unwrap() != upper {
                            frame += &format!(
                                "\x1B[38;5;{}m",
                                REVERSE_PALETTE[&(upper[0], upper[1], upper[2])]
                            );
                        }

                        if last_lower.is_none() || &last_lower.unwrap() != lower {
                            frame += &format!(
                                "\x1B[48;5;{}m",
                                REVERSE_PALETTE[&(lower[0], lower[1], lower[2])]
                            );
                        }
                    }
                    ColorMode::True => {
                        if last_upper.is_none() || &last_upper.unwrap() != upper {
                            frame += &format!(
                                "\x1B[38;2;{r};{g};{b}m",
                                r = upper[0],
                                g = upper[1],
                                b = upper[2]
                            );
                        }

                        if last_lower.is_none() || &last_lower.unwrap() != lower {
                            frame += &format!(
                                "\x1B[48;2;{r};{g};{b}m",
                                r = lower[0],
                                g = lower[1],
                                b = lower[2]
                            );
                        }
                    }
                }

                last_upper = Some(*upper);
                last_lower = Some(*lower);

                frame += "▀";
            }
            frame += "\n";
        }

        let bytes = frame.as_bytes();
        self.frame_lengths.push(bytes.len() as u64);
        self.frame_hashes.push(adler32(&bytes));
        self.output.write_all(bytes)
    }

    pub fn finish(self) -> io::Result<(Vec<u64>, Vec<u32>, fs::File)> {
        Ok((self.frame_lengths, self.frame_hashes, self.output.finish()?))
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
