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

pub trait Encoder {
    fn needs_width(&self) -> u32;
    fn needs_height(&self) -> u32;
    fn needs_color(&self) -> ColorMode;

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

    fn encode_frame(&self, img: &RgbImage) -> String {
        let mut last_upper: Option<Rgb<u8>> = None;
        let mut last_lower: Option<Rgb<u8>> = None;

        let mut frame = String::with_capacity((img.width() * img.height()) as usize);
        for y in (0..img.height() - 1).step_by(2) {
            for x in 0..img.width() {
                let upper = img.get_pixel(x, y);
                let lower = img.get_pixel(x, y + 1);

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
}

pub trait IOEncoder<W: Write>: Encoder {
    fn write_frame(&mut self, img: &RgbImage) -> io::Result<()>;
    fn finish(self) -> io::Result<(Vec<u64>, Vec<u32>, W)>;
}

pub struct FileEncoder<'a> {
    pub needs_width: u32,
    pub needs_height: u32,
    pub needs_color: ColorMode,
    pub frame_lengths: Vec<u64>,
    pub frame_hashes: Vec<u32>,
    pub output: OutputStream<'a>,
}

impl Encoder for FileEncoder<'_> {
    fn needs_color(&self) -> ColorMode {
        self.needs_color
    }

    fn needs_height(&self) -> u32 {
        self.needs_height
    }

    fn needs_width(&self) -> u32 {
        self.needs_width
    }
}

impl IOEncoder<fs::File> for FileEncoder<'_> {
    fn write_frame(&mut self, img: &RgbImage) -> io::Result<()> {
        let frame = self.encode_frame(img);
        let bytes = frame.as_bytes();
        self.frame_lengths.push(bytes.len() as u64);
        self.frame_hashes.push(adler32(&bytes));
        self.output.write_all(bytes)
    }

    fn finish(self) -> io::Result<(Vec<u64>, Vec<u32>, fs::File)> {
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
