pub mod ffmpeg;
pub mod tool_utils;
pub mod video_encoder;
#[cfg(feature = "cuda")]
pub mod cuda;

use std::collections::VecDeque;

use bytes::BytesMut;
use container::{
    bytes_hacking,
    codec::PacketEncoder,
    metadata::VideoMetadata,
    packet::{Packet, PacketMapper},
    ZSTD_UNCOMPRESSED_LEN_KEY,
};
pub use ffmpeg::*;
pub mod cli;
pub mod pre_processor;
use futures::SinkExt;
pub use pre_processor::*;
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    time::Instant,
};
use tokio_util::codec::FramedWrite;

pub trait PullSource {
    type Err;
    type Output;

    fn pull(&mut self) -> Result<Option<Self::Output>, Self::Err>;
}

pub trait Transformer {
    type Src;
    type Output;
    type Err;

    fn push(&mut self, src: &Self::Src) -> Result<(), Self::Err>;
    fn pull(&mut self) -> Result<Option<Self::Output>, Self::Err>;
    fn handle_input_close(&mut self) -> Result<(), Self::Err> {
        Ok(())
    }
}

pub async fn write_with_container_metadata(
    metadata: VideoMetadata,
    mut output: impl AsyncWrite + Unpin,
    mut receiver: tokio::sync::mpsc::Receiver<Packet<BytesMut>>,
    status_output: tokio::sync::watch::Sender<(f64, u64)>, // (rolling average fps, frame no)
    packet_mapper: impl PacketMapper,
) -> std::io::Result<()> {
    let metadata = rmp_serde::to_vec(&metadata).unwrap();
    output
        .write_all(&(metadata.len() as u64).to_le_bytes())
        .await?;
    output.write_all(&metadata).await?;
    output.flush().await?;

    let mut start_time = Instant::now();
    let mut times: VecDeque<f64> = VecDeque::new();

    let mut writer = FramedWrite::new(output, PacketEncoder::with_mapper(packet_mapper));

    let mut idx = 0u64;

    while let Some(packet) = receiver.recv().await {
        writer.feed(packet).await?;

        times.push_back(start_time.elapsed().as_secs_f64());
        if times.len() > 200 {
            times.pop_front();
        }

        let avg: f64 = times.len() as f64 / times.iter().sum::<f64>();
        status_output.send_modify(|(a, b)| {
            *a = avg;
            *b = idx
        });

        start_time = Instant::now();

        idx += 1;
    }

    status_output.send_modify(|(v, _)| *v = f64::NAN);

    writer.flush().await?;

    Ok(())
}

#[cfg(feature = "compression")]
pub struct PacketCompressor {
    compressor: zstd::bulk::Compressor<'static>,
}

#[cfg(feature = "compression")]
impl PacketCompressor {
    pub fn with_level(u: i32) -> std::io::Result<PacketCompressor> {
        let mut compressor = zstd::bulk::Compressor::new(u)?;
        compressor.include_magicbytes(false)?;
        compressor.include_contentsize(false)?;
        Ok(PacketCompressor { compressor })
    }
}

#[cfg(feature = "compression")]
impl PacketMapper for PacketCompressor {
    fn map_packet(&mut self, packet: &mut Packet<BytesMut>) -> Result<(), std::io::Error> {
        packet
            .extra_data
            .insert(ZSTD_UNCOMPRESSED_LEN_KEY, packet.data.len() as u32);

        let buffer = self.compressor.compress(&packet.data)?;
        packet.data = unsafe { bytes_hacking::bytesmut_from_vec(buffer) };

        Ok(())
    }
}

