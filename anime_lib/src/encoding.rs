use super::{
    metadata::{ColorMode, CompressionMode, DitherMode},
    palette::REVERSE_PALETTE,
    pattern,
};
use fast_image_resize as fr;
use image::{buffer::ConvertBuffer, imageops, Rgb, RgbImage, RgbaImage};
use simd_adler32::adler32;
use std::collections::HashSet;

use std::num::NonZeroU32;
use std::time::Duration;

/// Options for the encoder writing this packet into a sink / file.
#[derive(Copy, Clone, Debug)]
pub struct PacketFlags {
    pub compression_mode: CompressionMode,
    pub compression_level: Option<i32>,
    pub is_keyframe: bool,
}

/// The base-level data unit, representing a single frame of video or subtitle data.
#[derive(Debug, Clone)]
pub struct EncodedPacket {
    /// Index of the stream this packet belongs to.
    pub stream_index: u32,
    /// Adler32 checksum of this packet's data
    pub checksum: u32,
    /// Length of this packet's data in bytes
    pub length: u64,
    /// Presentation time of packet
    pub time: Duration,
    /// Duration of packet, if available
    pub duration: Option<Duration>,
    /// Packet data
    pub data: Vec<u8>,
    /// Options for the encoder writing this packet into a sink / file.
    pub encoder_opts: Option<PacketFlags>,
}

impl EncodedPacket {
    /// Create a packet from data, adding in a calculated checksum.
    pub fn from_data(
        stream_index: u32,
        time: Duration,
        duration: Option<Duration>,
        data: Vec<u8>,
        encoder_opts: Option<PacketFlags>,
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

    /// Switch the data in the packet, optionally re-calculating the checksum.
    pub fn switch_data(&mut self, data: Vec<u8>, refresh_checksum: bool) {
        if refresh_checksum {
            self.checksum = adler32(&data.as_slice());
        }

        self.length = data.len() as u64;
        self.data = data;
    }
}

/// A processor that resizes and dithers images as needed.
pub struct ProcessorPipeline {
    pub filter: fr::FilterType,
    pub width: u32,
    pub height: u32,
    pub dither_modes: HashSet<DitherMode>,
}

impl ProcessorPipeline {
    /// Process an image, returning a vector with resized versions of it in every color mode requested.
    pub fn process(&self, img: &RgbaImage) -> Vec<(DitherMode, RgbImage)> {
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

        let mut res = Vec::with_capacity(self.dither_modes.len());

        for mode in &self.dither_modes {
            match *mode {
                DitherMode::FloydSteinberg(map) => {
                    let mut dframe = frame.clone();
                    imageops::dither(&mut dframe, &map);
                    res.push((*mode, dframe));
                }
                DitherMode::Pattern(map, size, multiplier) => {
                    let mut dframe = frame.clone();
                    pattern::dither(&mut dframe, size, multiplier as f32 / 10_000.0, map);
                    res.push((*mode, dframe));
                }
                DitherMode::None => {
                    res.push((*mode, frame.clone()));
                }
            }
        }

        res
    }
}

/// A base trait for any ANSI image frame encoder, automatically implementing most of the encoding based on a few getter methods.
pub trait AnsiEncoder {
    fn color(&self, pixel: &Rgb<u8>, fg: bool) -> String {
        match self.needs_color() {
            ColorMode::EightBit => {
                if fg {
                    format!("\x1B[38;5;{}m", REVERSE_PALETTE[&pixel.0])
                } else {
                    format!("\x1B[48;5;{}m", REVERSE_PALETTE[&pixel.0])
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

    fn encode_frame(&mut self, image: &RgbImage) -> (String, u32) {
        let mut last_upper: Option<Rgb<u8>> = None;
        let mut last_lower: Option<Rgb<u8>> = None;
        let mut instructions = 0;

        let mut frame = String::with_capacity((image.width() * image.height()) as usize);
        for y in (0..image.height() - 1).step_by(2) {
            for x in 0..image.width() {
                let upper = image.get_pixel(x, y);
                let lower = image.get_pixel(x, y + 1);

                if last_upper.is_none() || &last_upper.unwrap() != upper {
                    frame += &self.color(upper, true);
                    instructions += 1;
                }

                if last_lower.is_none() || &last_lower.unwrap() != lower {
                    frame += &self.color(lower, false);
                    instructions += 1;
                }

                frame += "▀";

                last_upper = Some(*upper);
                last_lower = Some(*lower);
            }
            frame += "\x1b[1E";
            instructions += 1;
        }

        (frame, instructions)
    }

    fn needs_width(&self) -> u32;
    fn needs_height(&self) -> u32;
    fn needs_color(&self) -> ColorMode;
    fn needs_dither(&self) -> DitherMode {
        DitherMode::None
    }
}

/// A transformer that takes an object and converts it into an [EncodedPacket] if possible; else returning none.
pub trait PacketTransformer<'a> {
    type Source;
    fn encode_packet(&mut self, src: &Self::Source) -> Option<EncodedPacket>;
}

/// A transformer that takes an [EncodedPacket] and converts it into an object if possible; else returning none.
pub trait PacketDecoder {
    type Output;
    fn decode_packet(&mut self, src: EncodedPacket) -> Option<Self::Output>;
}
