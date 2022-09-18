mod codec;
pub use codec::*;
// pub mod midi;
pub mod subtitles;

use anime_telnet::encoding::{AnsiEncoder, EncodedPacket, PacketFlags, PacketTransformer};
use anime_telnet::metadata::{ColorMode, DitherMode};
use anime_telnet::pattern;
use futures::stream::Stream;
use image::{imageops, Rgb, RgbImage};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use cyanotype::{PixelFormat, VideoFrame, VideoFrameScaler};

/// An ANSI video encoder with a progress bar.
pub struct SpinnyANSIVideoEncoder {
    pub underlying: ANSIVideoEncoder,
    bar: ProgressBar,
}

impl SpinnyANSIVideoEncoder {
    pub fn from_underlying(
        underlying: ANSIVideoEncoder,
        parent: &MultiProgress,
    ) -> SpinnyANSIVideoEncoder {
        let bar = parent.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner} Stream {msg} @ {per_sec:5!}fps - encoding frame {pos}"),
        );
        bar.set_position(0);
        bar.set_message(underlying.stream_index.to_string());
        bar.enable_steady_tick(200);

        SpinnyANSIVideoEncoder { underlying, bar }
    }
}

impl PacketTransformer<'_> for SpinnyANSIVideoEncoder {
    type Source = (Duration, RgbImage);

    fn encode_packet(&mut self, packet: &Self::Source) -> Option<EncodedPacket> {
        self.bar.inc(1);
        Some(EncodedPacket::from_data(
            self.underlying.stream_index,
            packet.0,
            None,
            self.underlying.encode_frame(&packet.1).0.into_bytes(),
            Some(self.underlying.encoder_opts),
        ))
    }
}

pub struct ANSIVideoEncoder {
    pub stream_index: u32,
    pub width: u32,
    pub height: u32,
    pub color_mode: ColorMode,
    pub dither_mode: DitherMode,
    pub diff: bool,
    pub encoder_opts: PacketFlags,
    pub last_frame: Option<RgbImage>,
}

impl AnsiEncoder for ANSIVideoEncoder {
    fn needs_color(&self) -> ColorMode {
        self.color_mode
    }

    fn needs_height(&self) -> u32 {
        self.height
    }

    fn needs_width(&self) -> u32 {
        self.width
    }

    fn needs_dither(&self) -> DitherMode {
        self.dither_mode
    }
}

impl ANSIVideoEncoder {
    fn encode_diffed_frame(&self, image: &RgbImage) -> (String, u32) {
        let mut last_upper: Option<Rgb<u8>> = None;
        let mut last_lower: Option<Rgb<u8>> = None;

        let mut last_x = 0;
        let mut instructions = 0;

        let mut frame = String::with_capacity((image.width() * image.height()) as usize);
        for y in (0..image.height() - 1).step_by(2) {
            for x in 0..image.width() {
                let upper = image.get_pixel(x, y);
                let lower = image.get_pixel(x, y + 1);
                if let Some(ref old_img) = self.last_frame {
                    if old_img.get_pixel(x, y) != upper || old_img.get_pixel(x, y + 1) != lower {
                        if last_x != x + 1 {
                            frame += &format!("\x1b[{}G", x + 1);
                            instructions += 1;
                        }

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

                        last_x = x;
                    }
                } else {
                    last_x = x;

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
            }

            frame += "\x1b[1E";
            instructions += 1;
            last_x = 0;
        }

        (frame, instructions)
    }

    fn encode_best(&mut self, image: &RgbImage) -> (String, bool) {
        if self.diff {
            let (non_diffed, non_diffed_instructions) = self.encode_frame(image);
            let (diffed, diffed_instructions) = self.encode_diffed_frame(image);

            self.last_frame = Some(image.clone());

            if non_diffed_instructions < diffed_instructions {
                (non_diffed, true)
            } else {
                (diffed, false)
            }
        } else {
            (self.encode_frame(image).0, true)
        }
    }
}

impl PacketTransformer<'_> for ANSIVideoEncoder {
    type Source = (Duration, RgbImage);

    fn encode_packet(&mut self, packet: &Self::Source) -> Option<EncodedPacket> {
        let (frame, keyframe) = self.encode_best(&packet.1);
        let mut flags = self.encoder_opts;
        flags.is_keyframe = keyframe;

        Some(EncodedPacket::from_data(
            self.stream_index,
            packet.0,
            None,
            frame.into_bytes(),
            Some(flags),
        ))
    }
}

pub struct FFMpegProcessor {
    scaler: VideoFrameScaler,
    pub width: u32,
    pub height: u32,
    pub dither_modes: HashSet<DitherMode>,
}

impl FFMpegProcessor {
    pub fn new(
        source_pixel_format: PixelFormat,
        dither_modes: impl Into<HashSet<DitherMode>>,
        source_width: usize,
        source_height: usize,
        target_width: usize,
        target_height: usize,
    ) -> FFMpegProcessor {
        let video_scaler = VideoFrameScaler::builder()
            .source_pixel_format(source_pixel_format)
            .source_width(source_width)
            .source_height(source_height)
            .target_pixel_format(PixelFormat::from_str("rgb24").unwrap())
            .target_width(target_width)
            .target_height(target_height)
            .algorithm(cyanotype::ac_ffmpeg::codec::video::scaler::Algorithm::Bicubic)
            .build()
            .unwrap();

        FFMpegProcessor {
            scaler: video_scaler,
            width: target_width as u32,
            height: target_height as u32,
            dither_modes: dither_modes.into(),
        }
    }

    /// Process an image, returning a vector with resized versions of it in every color mode requested.
    pub fn process(&mut self, img: &VideoFrame) -> Vec<(DitherMode, RgbImage)> {
        let img = self.scaler.scale(img).unwrap();
        let frame: RgbImage =
            RgbImage::from_raw(self.width, self.height, img.planes()[0].data().to_vec()).unwrap();

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

pub type BoxedPacketStream = Pin<Box<dyn Stream<Item = std::io::Result<EncodedPacket>>>>;
