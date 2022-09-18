use bytes::{Bytes, BytesMut};
use container::{
    bytes_hacking,
    packet::{Packet, PacketMapper},
    ZSTD_UNCOMPRESSED_LEN_KEY,
};

pub mod playing;
pub use playing::*;

pub mod subtitles;

#[cfg(feature = "compression")]
pub struct PacketDecompressor {
    decompressor: zstd::bulk::Decompressor<'static>,
}

#[cfg(feature = "compression")]
impl PacketDecompressor {
    pub fn new() -> std::io::Result<PacketDecompressor> {
        let mut decompressor = zstd::bulk::Decompressor::new()?;
        decompressor.include_magicbytes(false)?;
        Ok(PacketDecompressor { decompressor })
    }
}

#[cfg(feature = "compression")]
impl PacketMapper for PacketDecompressor {
    fn map_packet(&mut self, packet: &mut Packet<BytesMut>) -> Result<(), std::io::Error> {
        let decompressed_len = packet
            .extra_data
            .get(ZSTD_UNCOMPRESSED_LEN_KEY)
            .ok_or(std::io::Error::from(std::io::ErrorKind::InvalidData))?
            as usize;

        let buffer = self
            .decompressor
            .decompress(&packet.data, decompressed_len)?;
        packet.data = unsafe { bytes_hacking::bytesmut_from_vec(buffer) };

        Ok(())
    }
}

pub trait PacketFilterTransformer {
    fn filter_map_packet(&mut self, packet: Packet<Bytes>) -> Option<Packet<Bytes>>;
}

impl PacketFilterTransformer for () {
    fn filter_map_packet(&mut self, packet: Packet<Bytes>) -> Option<Packet<Bytes>> {
        Some(packet)
    }
}
