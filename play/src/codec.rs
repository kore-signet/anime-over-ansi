use anime_telnet::encoding::*;

use bytes::{Buf, BytesMut};

use simd_adler32::adler32;
use std::time::Duration;
use tokio::io;
use tokio_util::codec::Decoder;

/// A codec that converts bytes into EncodedPacket's, decompressing them if needed and checking the contents with the packet checksum.
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
                data = res;
            }

            if adler32(&data.as_slice()) != checksum {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid checksum for block",
                ));
            }

            data
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
