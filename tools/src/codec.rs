use anime_telnet::encoding::*;

use anime_telnet::metadata::CompressionMode;
use bytes::{Buf, BufMut, BytesMut};

use std::time::Duration;
use tokio_util::codec::{Decoder, Encoder};

pub struct PacketWriteCodec {
    compressor: zstd::block::Compressor,
}

impl PacketWriteCodec {
    pub fn new() -> PacketWriteCodec {
        PacketWriteCodec {
            compressor: zstd::block::Compressor::new(),
        }
    }
}

impl Encoder<EncodedPacket> for PacketWriteCodec {
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

pub struct PacketReadCodec {
    decompressor: zstd::block::Decompressor,
    decode_data: bool,
}

impl PacketReadCodec {
    pub fn new(decode_data: bool) -> PacketReadCodec {
        PacketReadCodec {
            decompressor: zstd::block::Decompressor::new(),
            decode_data,
        }
    }
}

impl Decoder for PacketReadCodec {
    type Item = EncodedPacket;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 9 {
            // not enough for length marker
            return Ok(None);
        }

        let mut length_bytes = [0u8; 8];
        length_bytes.copy_from_slice(&src[..8]);
        let length = u64::from_be_bytes(length_bytes) as usize;

        let mut compression_marker_bytes = [0u8; 1];
        compression_marker_bytes.copy_from_slice(&src[8..9]);
        let compression = u8::from_be_bytes(compression_marker_bytes);

        let header_size = if compression != 0 { 41 } else { 33 };
        if src.len() < header_size + length {
            // not enough for full header + data
            src.reserve(header_size + length - src.len());
            return Ok(None);
        }

        src.advance(9);

        let uncompressed_size = if compression != 0 { src.get_u64() } else { 0 };
        let stream_index = src.get_u32();
        let checksum = src.get_u32();
        let time = Duration::from_nanos(src.get_u64());
        let duration = src.get_u64();
        let duration = if duration == u64::MAX {
            None
        } else {
            Some(Duration::from_nanos(duration))
        };

        let data = if self.decode_data {
            let mut data = vec![0; length];
            src.copy_to_slice(&mut data);

            if compression == 1 {
                let mut res = Vec::with_capacity(uncompressed_size as usize);
                self.decompressor
                    .decompress_to_buffer(&data, &mut res)
                    .unwrap();
                res
            } else {
                data
            }
        } else {
            src.advance(length);
            Vec::new()
        };

        Ok(Some(EncodedPacket {
            stream_index,
            checksum,
            length: length as u64,
            time,
            duration,
            data,
            encoder_opts: None,
        }))
    }
}
