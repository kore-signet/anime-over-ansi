mod codec;
pub use codec::*;
// pub mod midi;
pub mod subtitles;

use anime_telnet::encoding::{AnsiEncoder, EncodedPacket, PacketFlags, PacketTransformer};
use anime_telnet::metadata::{ColorMode, DitherMode};
use futures::stream::Stream;
use image::{Rgb, RgbImage};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::pin::Pin;

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
    type Source = cyanotype::VideoPacket<RgbImage>;

    fn encode_packet(&mut self, packet: &Self::Source) -> Option<EncodedPacket> {
        self.bar.inc(1);
        Some(EncodedPacket::from_data(
            self.underlying.stream_index,
            packet.time,
            None,
            self.underlying.encode_frame(&packet.frame).0.into_bytes(),
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

            frame += &"\x1b[1E".to_string();
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
    type Source = cyanotype::VideoPacket<RgbImage>;

    fn encode_packet(&mut self, packet: &Self::Source) -> Option<EncodedPacket> {
        let (frame, keyframe) = self.encode_best(&packet.frame);
        let mut flags = self.encoder_opts;
        flags.is_keyframe = keyframe;

        Some(EncodedPacket::from_data(
            self.stream_index,
            packet.time,
            None,
            frame.into_bytes(),
            Some(flags),
        ))
    }
}

pub type BoxedPacketStream = Pin<Box<dyn Stream<Item = std::io::Result<EncodedPacket>>>>;
