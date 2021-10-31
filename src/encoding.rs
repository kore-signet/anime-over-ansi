use super::{metadata::ColorMode, palette::REVERSE_PALETTE};
use image::{
    imageops::{self},
    Rgb, RgbImage,
};
use std::fs;
use std::io::{self, Write};

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
    pub output: OutputStream<'a>,
}

impl Encoder<'_> {
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
        self.output.write_all(bytes)
    }

    pub fn finish(self) -> io::Result<(Vec<u64>, fs::File)> {
        Ok((self.frame_lengths, self.output.finish()?))
    }
}

pub struct ResizePipeline {
    pub filter: imageops::FilterType,
    pub width: u32,
    pub height: u32,
    pub last_frame: Option<RgbImage>,
}

impl ResizePipeline {
    pub fn resize(&mut self, img: &RgbImage) {
        self.last_frame = Some(imageops::resize(img, self.width, self.height, self.filter));
    }

    pub fn last_frame(&self) -> &RgbImage {
        self.last_frame.as_ref().unwrap()
    }
}
