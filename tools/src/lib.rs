mod codec;
// pub mod player;
pub use codec::*;
pub mod subtitles;

use anime_telnet::encoding::{AnsiEncoder, EncodedPacket, EncoderOptions, PacketTransformer};
use anime_telnet::metadata::{ColorMode, DitherMode};
use futures::stream::Stream;
use image::RgbImage;
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

impl PacketTransformer for SpinnyANSIVideoEncoder {
    type Source = cyanotype::VideoPacket<RgbImage>;

    fn encode_packet(&self, packet: &Self::Source) -> Option<EncodedPacket> {
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
    pub encoder_opts: EncoderOptions,
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

impl PacketTransformer for ANSIVideoEncoder {
    type Source = cyanotype::VideoPacket<RgbImage>;

    fn encode_packet(&self, packet: &Self::Source) -> Option<EncodedPacket> {
        Some(EncodedPacket::from_data(
            self.stream_index,
            packet.time,
            None,
            self.encode_frame(&packet.frame).0.into_bytes(),
            Some(self.encoder_opts),
        ))
    }
}

pub type BoxedPacketStream = Pin<Box<dyn Stream<Item = std::io::Result<EncodedPacket>>>>;
