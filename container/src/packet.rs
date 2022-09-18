use bytes::{Bytes, BytesMut};
use rend::LittleEndian;
use std::{convert::Infallible, time::Duration};

use crate::TinyMap;

#[derive(Debug, Clone)]
pub struct WirePacket {
    pub header: WirePacketHeader,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct WirePacketHeader {
    pub stream_index: LittleEndian<u16>,
    pub checksum: LittleEndian<u32>,
    pub presentation_length: LittleEndian<u64>, // nanoseconds
    pub presentation_time: LittleEndian<u64>,   // nanoseconds
    pub data_length: LittleEndian<u64>,
    pub extra_data_length: u16,
}

// replace with Bytes/BytesMut
#[derive(Clone, Debug)]
pub struct Packet<V: AsRef<[u8]>> {
    pub stream_index: u16,
    pub presentation_length: Duration,
    pub presentation_time: Duration,
    pub data: V,
    pub extra_data: TinyMap,
}

impl<V: AsRef<[u8]>> Packet<V> {
    pub fn wire_header(&self, hasher: &mut crc32fast::Hasher) -> WirePacketHeader {
        hasher.update(self.extra_data.serialize());
        hasher.update(self.data.as_ref());
        let checksum = hasher.clone().finalize();

        WirePacketHeader {
            stream_index: LittleEndian::from(self.stream_index),
            checksum: LittleEndian::from(checksum),
            presentation_length: LittleEndian::from(self.presentation_length.as_nanos() as u64),
            presentation_time: LittleEndian::from(self.presentation_time.as_nanos() as u64),
            data_length: LittleEndian::from(self.data.as_ref().len() as u64),
            extra_data_length: self.extra_data.serialize().len() as u16,
        }
    }

    pub fn from_wire(header: WirePacketHeader, data: V, extra_data: V) -> Packet<V> {
        Packet {
            stream_index: header.stream_index.value(),
            presentation_length: Duration::from_nanos(header.presentation_length.value()),
            presentation_time: Duration::from_nanos(header.presentation_time.value()),
            data,
            extra_data: TinyMap::from_bytes(extra_data.as_ref()).unwrap(),
        }
    }
}

impl Packet<BytesMut> {
    pub fn freeze(self) -> Packet<Bytes> {
        Packet {
            stream_index: self.stream_index,
            presentation_length: self.presentation_length,
            presentation_time: self.presentation_time,
            data: self.data.freeze(),
            extra_data: self.extra_data,
        }
    }
}

/// A transformer that takes an object and converts it into a [Packet] if possible; else returning none.
pub trait ToPacket {
    type Source;
    type Err;

    fn encode_packet(&mut self, src: Self::Source) -> Result<Packet<BytesMut>, Self::Err>;
}

/// A transformer that takes a [Packet] and converts it into an object if possible; else returning none.
pub trait PacketDecoder {
    type Output;
    fn decode_packet(&mut self, src: Packet<Bytes>) -> Option<Self::Output>;
}

pub trait PacketMapper {
    fn map_packet(&mut self, packet: &mut Packet<BytesMut>) -> Result<(), std::io::Error>;
}

impl PacketMapper for () {
    #[inline]
    fn map_packet(&mut self, _: &mut Packet<BytesMut>) -> Result<(), std::io::Error> {
        Ok(())
    }
}

impl ToPacket for () {
    type Source = Packet<BytesMut>;
    type Err = Infallible;

    fn encode_packet(&mut self, src: Self::Source) -> Result<Packet<BytesMut>, Self::Err> {
        Ok(src)
    }
}

impl PacketMapper for Box<dyn PacketMapper> {
    fn map_packet(&mut self, packet: &mut Packet<BytesMut>) -> Result<(), std::io::Error> {
        self.as_mut().map_packet(packet)
    }
}
