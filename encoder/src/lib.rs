use anime_telnet::encoding::*;

use anime_telnet::{
    metadata::{ColorMode, CompressionMode},
    palette::REVERSE_PALETTE,
};
use bytes::{BufMut, BytesMut};
use image::{Rgb, RgbImage};
use std::sync::Mutex;
use tokio_util::codec::Encoder;

pub struct PacketCodec {
    compressor: zstd::block::Compressor,
}

impl PacketCodec {
    pub fn new() -> PacketCodec {
        PacketCodec {
            compressor: zstd::block::Compressor::new(),
        }
    }
}

impl Encoder<EncodedPacket> for PacketCodec {
    type Error = std::io::Error;
    fn encode(&mut self, mut v: EncodedPacket, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let uncompressed_size = v.length;

        if let Some(opts) = v.encoder_opts {
            if opts.compression_mode == CompressionMode::Zstd {
                v.switch_data(
                    self.compressor
                        .compress(&v.data, opts.compression_level.unwrap())
                        .unwrap(),
                );
            }
        }

        dst.put_u64(v.length);
        dst.put_u8(
            v.encoder_opts
                .map(|v| v.compression_mode as u8)
                .unwrap_or(0),
        );
        if v.encoder_opts
            .map(|v| v.compression_mode)
            .unwrap_or(CompressionMode::None)
            == CompressionMode::Zstd
        {
            dst.put_u64(uncompressed_size);
        }

        dst.put_u32(v.stream_index);
        dst.put_u32(v.checksum);
        dst.put_u64(v.time.as_nanos() as u64);
        dst.put_u64(v.duration.map(|v| v.as_nanos() as u64).unwrap_or(u64::MAX));
        dst.put_slice(&v.data);

        Ok(())
    }
}

pub struct ANSIVideoEncoder {
    pub stream_index: u32,
    pub width: u32,
    pub height: u32,
    pub color_mode: ColorMode,
    pub encoder_opts: EncoderOptions,
}

impl ANSIVideoEncoder {
    fn color(&self, pixel: &Rgb<u8>, fg: bool) -> String {
        match self.color_mode {
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

    pub fn encode_frame(&self, packet: &cyanotype::VideoPacket<RgbImage>) -> EncodedPacket {
        let mut last_upper: Option<Rgb<u8>> = None;
        let mut last_lower: Option<Rgb<u8>> = None;

        let mut frame =
            String::with_capacity((packet.frame.width() * packet.frame.height()) as usize);
        for y in (0..packet.frame.height() - 1).step_by(2) {
            for x in 0..packet.frame.width() {
                let upper = packet.frame.get_pixel(x, y);
                let lower = packet.frame.get_pixel(x, y + 1);

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

        EncodedPacket::from_data(
            self.stream_index,
            packet.time,
            None,
            frame.into_bytes(),
            Some(self.encoder_opts),
        )
    }
}
