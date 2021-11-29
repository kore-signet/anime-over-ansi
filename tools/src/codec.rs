use anime_telnet::encoding::*;

use anime_telnet::metadata::CompressionMode;
use bytes::{BufMut, BytesMut};

use tokio_util::codec::Encoder;

/// A codec that converts EncodedPacket's into bytes, compressing them if needed.
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
                    false, // don't refresh checksum
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
