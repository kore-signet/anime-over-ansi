#![allow(unused_assignments)]

use anime_telnet::encoding::EncodedPacket;



use tokio::{
    time::{Duration},
};

use bytes::{Buf, BytesMut};
use tokio_util::codec::Decoder;

pub struct PacketDecoder {
    decompressor: zstd::block::Decompressor,
    decode_data: bool,
}

impl PacketDecoder {
    pub fn new(decode_data: bool) -> PacketDecoder {
        PacketDecoder {
            decompressor: zstd::block::Decompressor::new(),
            decode_data,
        }
    }
}

impl Decoder for PacketDecoder {
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

// pub async fn play<T>(
//     mut reader: Pin<&mut T>,
//     frame_lengths: Vec<u64>,
//     frame_hashes: Vec<u32>,
//     frame_times: Vec<i64>,
//     subtitles: &mut Vec<SubtitleEntry>,
//     tx: broadcast::Sender<Vec<u8>>,
// ) -> anyhow::Result<()>
// where
//     T: AsyncRead,
// {
//     let start = Instant::now();
//     let mut current = Duration::from_nanos(0);
//     let mut next_subtitle = subtitles.pop();

//     // print!("\x1B[2J\x1B[1;1H");

//     let mut i = 0;

//     while i < frame_lengths.len() - 1 {
//         current = Duration::from_nanos(frame_times[i] as u64);

//         let mut next_frame = vec![0; frame_lengths[i] as usize];
//         reader.read_exact(&mut next_frame).await?;
//         let hash = adler32(&next_frame.as_slice());
//         if hash != frame_hashes[i] {
//             panic!("detected corrupted data at frame {}", i);
//         }

// tx.send(.to_vec())?;
//         tx.send(next_frame)?;

//         if let Some(ref next_s) = next_subtitle {
//             let start_time = Duration::from_millis(next_s.timespan.start.abs().msecs() as u64);

//             if current >= start_time {
//                 tx.send(b"\x1B[0m\x1B[0J".to_vec())?;
//                 tx.send(
//                     next_s
//                         .line
//                         .as_ref()
//                         .unwrap()
//                         .replace("\n", " ")
//                         .as_bytes()
//                         .to_vec(),
//                 )?;
//             }

//             let end_time = Duration::from_millis(next_s.timespan.end.abs().msecs() as u64);

//             if current >= end_time {
//                 next_subtitle = subtitles.pop();
//             }
//         }

//         i += 1;

//         sleep_until(start + Duration::from_nanos(frame_times[i + 1] as u64)).await;
//     }

//     Ok(())
// }
