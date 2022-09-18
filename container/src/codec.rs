use std::io;
use std::slice;

use crate::packet::*;
use bytes::Buf;
use bytes::Bytes;
use bytes::BytesMut;
use tokio_util::codec::Decoder as TokioDecoder;
use tokio_util::codec::Encoder as TokioEncoder;

pub const HEADER_LEN: usize = std::mem::size_of::<WirePacketHeader>();

pub struct PacketEncoder<T: PacketMapper> {
    mapper: Option<T>,
    hasher: crc32fast::Hasher,
}

impl PacketEncoder<()> {
    pub fn passthrough() -> PacketEncoder<()> {
        PacketEncoder {
            mapper: None,
            hasher: crc32fast::Hasher::new(),
        }
    }
}

impl<T: PacketMapper> PacketEncoder<T> {
    pub fn with_mapper(mapper: T) -> PacketEncoder<T> {
        PacketEncoder {
            mapper: Some(mapper),
            hasher: crc32fast::Hasher::new(),
        }
    }
}

impl<T: PacketMapper> TokioEncoder<Packet<BytesMut>> for PacketEncoder<T> {
    type Error = std::io::Error;

    fn encode(
        &mut self,
        mut item: Packet<BytesMut>,
        dst: &mut bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        if let Some(ref mut mapper) = self.mapper {
            mapper.map_packet(&mut item)?;
        }

        dst.reserve(HEADER_LEN + item.data.len());

        let wire_header = item.wire_header(&mut self.hasher);
        self.hasher.reset();

        unsafe {
            dst.extend_from_slice(slice::from_raw_parts(
                (&wire_header) as *const WirePacketHeader as *const u8,
                HEADER_LEN,
            ))
        }

        if !item.extra_data.is_empty() {
            dst.extend_from_slice(item.extra_data.serialize());
        }

        dst.extend_from_slice(&item.data);

        Ok(())
    }
}

pub struct PacketDecoder<T: PacketMapper> {
    mapper: Option<T>,
    hasher: Option<crc32fast::Hasher>,
}

impl PacketDecoder<()> {
    pub fn passthrough() -> PacketDecoder<()> {
        PacketDecoder {
            mapper: None,
            hasher: Some(crc32fast::Hasher::new()),
        }
    }
}

impl<T: PacketMapper> PacketDecoder<T> {
    pub fn with_mapper(mapper: T) -> PacketDecoder<T> {
        PacketDecoder {
            mapper: Some(mapper),
            hasher: Some(crc32fast::Hasher::new()),
        }
    }
}

impl<T: PacketMapper> TokioDecoder for PacketDecoder<T> {
    type Error = std::io::Error;
    type Item = Packet<Bytes>;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < HEADER_LEN {
            return Ok(None);
        }

        let header = unsafe { &*(src.as_ref()[..HEADER_LEN].as_ptr() as *const WirePacketHeader) };

        if src.len()
            < HEADER_LEN + header.data_length.value() as usize + header.extra_data_length as usize
        {
            src.reserve(
                header.data_length.value() as usize
                    + header.extra_data_length as usize
                    + HEADER_LEN
                    - src.len(),
            );
            return Ok(None);
        }

        // copy header; advance byte buffer
        let header = *header;
        src.advance(HEADER_LEN);

        let extra_data: BytesMut = if header.extra_data_length > 0 {
            src.split_to(header.extra_data_length as usize)
        } else {
            BytesMut::new()
        };

        let data = src.split_to(header.data_length.value() as usize);

        if let Some(ref mut checker) = self.hasher {
            checker.update(&extra_data);
            checker.update(&data);
            let hash = checker.clone().finalize();
            checker.reset();

            if hash != header.checksum.value() {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
        }

        let mut packet = Packet::from_wire(header, data, extra_data);
        if let Some(ref mut mapper) = self.mapper {
            mapper.map_packet(&mut packet)?;
        }

        Ok(Some(packet.freeze()))
    }
}
