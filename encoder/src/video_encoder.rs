use std::{convert::Infallible, time::Duration};

use bytes::BytesMut;
use container::{
    metadata::ColorMode,
    packet::{Packet, ToPacket},
    TinyMap, VideoPacketFlags, VIDEO_PACKET_KEY,
};
use enumflags2::{make_bitflags, BitFlags};
use image::RgbImage;
use img2ansi::AnsiEncoder;

pub struct FrameEncoder {
    pub stream_index: u16,
    pub width: u32,
    pub height: u32,
    pub color: ColorMode,
    pub use_diffing: bool,
    pub last_frame: Option<RgbImage>,
}

impl AnsiEncoder for FrameEncoder {
    fn needs_width(&self) -> u32 {
        self.width
    }

    fn needs_height(&self) -> u32 {
        self.height
    }

    fn needs_color(&self) -> ColorMode {
        self.color
    }

    fn replace_last_frame(&mut self, new_frame: RgbImage) -> Option<RgbImage> {
        self.last_frame.replace(new_frame)
    }

    fn use_diffing(&self) -> bool {
        self.use_diffing
    }
}

pub struct DecodedVideoFrame {
    pub pts: Duration,
    pub duration: Duration,
    pub image: RgbImage,
}

impl ToPacket for FrameEncoder {
    type Source = DecodedVideoFrame;
    type Err = Infallible;

    fn encode_packet(&mut self, src: Self::Source) -> Result<Packet<BytesMut>, Infallible> {
        let (encoded, keyframe) = self.encode_best(&src.image);
        let flags: BitFlags<VideoPacketFlags> = if keyframe {
            make_bitflags!(VideoPacketFlags::{Keyframe})
        } else {
            BitFlags::empty()
        };

        let mut extra_data = TinyMap::new();
        extra_data.insert(VIDEO_PACKET_KEY, flags.bits());

        Ok(Packet {
            stream_index: self.stream_index,
            presentation_length: src.duration,
            presentation_time: src.pts,
            data: encoded,
            extra_data,
        })
    }
}
