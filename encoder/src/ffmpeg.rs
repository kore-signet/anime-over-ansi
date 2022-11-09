use std::convert::Infallible;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::{PullSource, Transformer};
use img2ansi::VideoImage;

use super::video_encoder::DecodedVideoFrame;
use ac_ffmpeg::codec::video::scaler::Algorithm;
use ac_ffmpeg::codec::video::PixelFormat;
use ac_ffmpeg::codec::Decoder;
use ac_ffmpeg::time::TimeBase;
use ac_ffmpeg::Error as FFMPEGError;
use ac_ffmpeg::{
    codec::video::{VideoDecoder, VideoFrameScaler},
    format::{
        demuxer::{Demuxer, DemuxerWithStreamInfo},
        stream::Stream,
    },
};
use bytes::BytesMut;
use container::packet::Packet as MoePacket;
use image::RgbImage;
use postage::sink::Sink;
use tokio::task::JoinError;

pub type FFMPEGResult<T> = Result<T, ac_ffmpeg::Error>;

#[derive(Clone)]
pub struct FFMpegPacket(pub usize, pub ac_ffmpeg::packet::Packet);

pub struct FFMpegSource {
    dmxr: DemuxerWithStreamInfo<()>,
}

impl FFMpegSource {
    pub fn open_url(url: &str) -> FFMPEGResult<FFMpegSource> {
        let demuxer = Demuxer::builder()
            .build_from_url(url)?
            .find_stream_info(None)
            .map_err(|(_, e)| e)?;

        Ok(FFMpegSource { dmxr: demuxer })
    }

    pub fn streams(&self) -> &[Stream] {
        self.dmxr.streams()
    }

    // gets name of formats we're demuxing
    pub fn get_format_names(&self) -> Option<&'static str> {
        self.dmxr.get_format_names()
    }
}

impl PullSource for FFMpegSource {
    type Err = ac_ffmpeg::Error;
    type Output = FFMpegPacket;

    fn pull(&mut self) -> FFMPEGResult<Option<FFMpegPacket>> {
        if let Some(v) = self.dmxr.take()? {
            return Ok(Some(FFMpegPacket(v.stream_index(), v)));
        }

        Ok(None)
    }
}

/*
ffmpeg video decoder
*/

pub struct FFMpegVideoDecoder {
    decoder: VideoDecoder,
    scaler: VideoFrameScaler,
    target_width: usize,
    target_height: usize,
    time_base: TimeBase,
    scaling_algorithm: Algorithm,
}

impl FFMpegVideoDecoder {
    pub fn from_stream(
        stream: &ac_ffmpeg::format::stream::Stream,
        scaling_algorithm: Algorithm,
        target_width: usize,
        target_height: usize,
    ) -> FFMPEGResult<FFMpegVideoDecoder> {
        let parameters = stream.codec_parameters();
        let video_parameters = parameters.as_video_codec_parameters().unwrap();

        let decoder = VideoDecoder::from_stream(stream)?.build()?;
        let scaler = VideoFrameScaler::builder()
            .source_pixel_format(video_parameters.pixel_format())
            .source_height(video_parameters.height())
            .source_width(video_parameters.width())
            .target_height(target_height)
            .target_width(target_width)
            .target_pixel_format(PixelFormat::from_str("rgb24").unwrap())
            .algorithm(scaling_algorithm)
            .build()?;

        Ok(FFMpegVideoDecoder {
            decoder,
            scaler,
            target_height,
            target_width,
            time_base: stream.time_base(),
            scaling_algorithm,
        })
    }
}

impl Clone for FFMpegVideoDecoder {
    fn clone(&self) -> Self {
        let video_parameters = self.decoder.codec_parameters();
        let decoder = VideoDecoder::from_codec_parameters(&video_parameters)
            .unwrap()
            .time_base(self.time_base)
            .build()
            .unwrap();
        let scaler = VideoFrameScaler::builder()
            .source_pixel_format(video_parameters.pixel_format())
            .source_height(video_parameters.height())
            .source_width(video_parameters.width())
            .target_height(self.target_height)
            .target_width(self.target_width)
            .target_pixel_format(PixelFormat::from_str("rgb24").unwrap())
            .algorithm(self.scaling_algorithm)
            .build()
            .unwrap();

        Self {
            decoder,
            scaler,
            target_width: self.target_width,
            target_height: self.target_height,
            time_base: self.time_base,
            scaling_algorithm: self.scaling_algorithm,
        }

        // Self { decoder: self.decoder.clone(), scaler: self.scaler.clone(), target_width: self.target_width.clone(), target_height: self.target_height.clone(), time_base: self.time_base.clone() }
    }
}

impl Transformer for FFMpegVideoDecoder {
    type Src = FFMpegPacket;
    type Output = DecodedVideoFrame;
    type Err = FFMPEGError;

    fn push(&mut self, src: &Self::Src) -> Result<(), Self::Err> {
        self.decoder.push(src.1.clone())
    }

    fn pull(&mut self) -> Result<Option<Self::Output>, Self::Err> {
        self.pull_from_decoder()
    }

    fn handle_input_close(&mut self) -> Result<(), Self::Err> {
        self.decoder.flush()
    }
}

impl FFMpegVideoDecoder {
    fn pull_from_decoder(&mut self) -> FFMPEGResult<Option<DecodedVideoFrame>> {
        if let Some(ref v) = self.decoder.take()? {
            let v = self.scaler.scale(v)?;

            let pts = Duration::from_nanos(
                v.pts()
                    .as_nanos()
                    .map(|v| v as u64)
                    .ok_or(FFMPEGError::new("packet missing timestamp"))?,
            );
            let duration = v
                .packet_duration()
                .as_nanos()
                .map(|v| Duration::from_nanos(v as u64))
                .unwrap_or_default();
            let image = RgbImage::from_vec(
                self.target_width as u32,
                self.target_height as u32,
                v.planes()[0].data().to_vec(),
            )
            .ok_or(FFMPEGError::new("invalid videoframe data"))?;

            return Ok(Some(DecodedVideoFrame {
                pts,
                duration,
                image: VideoImage::FullColor(image),
            }));
        }

        Ok(None)
    }
}

/*
decoding for generic packets
*/

pub struct GenericPacketDecoder {
    new_stream_index: u16,
    buffer: Option<MoePacket<BytesMut>>,
}

impl GenericPacketDecoder {
    pub fn override_stream_index(idx: u16) -> Self {
        Self {
            new_stream_index: idx,
            buffer: None,
        }
    }
}

impl Transformer for GenericPacketDecoder {
    type Src = FFMpegPacket;
    type Output = MoePacket<BytesMut>;
    type Err = Infallible;

    fn push(&mut self, src: &Self::Src) -> Result<(), Self::Err> {
        self.buffer = Some(MoePacket {
            stream_index: self.new_stream_index,
            presentation_length: src
                .1
                .duration()
                .as_nanos()
                .map(|v| Duration::from_nanos(v as u64))
                .unwrap_or_default(),
            presentation_time: src
                .1
                .pts()
                .as_nanos()
                .map(|v| Duration::from_nanos(v as u64))
                .unwrap_or_default(),
            data: BytesMut::from(src.1.data()),
            extra_data: Default::default(),
        });

        Ok(())
    }

    fn pull(&mut self) -> Result<Option<Self::Output>, Self::Err> {
        Ok(self.buffer.take())
    }
}

/*
cursed macros
*/

#[macro_export]
macro_rules! pipeline {
    (receive from $rx:expr; send to $tx:expr; stream $stream_index:expr => $transformer:expr => passthrough => passthrough) => {
        pipeline!(receive from $rx; send to $tx; stream $stream_index => $transformer => () => ())
    };
    (receive from $rx:expr; send to $tx:expr; stream $stream_index:expr => $transformer:expr => $processor:expr => passthrough) => {
        pipeline!(receive from $rx; send to $tx; stream $stream_index => $transformer => $processor => ())
    };
    (receive from $rx:expr; send to $tx:expr; stream $stream_index:expr => $transformer:expr => passthrough => $encoder:expr) => {
        pipeline!(receive from $rx; send to $tx; stream $stream_index => $transformer => () => $encoder)
    };
    (receive from $rx:expr; send to $tx:expr; stream $stream_index:expr => $transformer:expr => $processor:expr => $encoder:expr) => {
        {
            let mut rx = $rx.clone().filter(move |v| v.0 == $stream_index);
            let mut tx = $tx.clone();
            let mut transformer = $transformer;
            let mut processor = $processor;
            let mut encoder = $encoder;

            let res: Box<dyn FnMut() -> FFMPEGResult<()> + Send> = Box::new(move || -> FFMPEGResult<()> {
                let mut input_closed = false;
                loop {
                    let res = transformer.pull();
                    if let Some(mut v) = res.unwrap() {
                        processor.map(&mut v);
                        tx.blocking_send(encoder.encode_packet(v).unwrap());
                        continue;
                    } else if input_closed {
                        break;
                    }

                    use postage::stream::TryRecvError;

                    match rx.try_recv() {
                        Ok(packet) => {
                            transformer.push(&packet).unwrap();
                        }
                        Err(TryRecvError::Pending) => {
                            continue;
                        }
                        Err(TryRecvError::Closed) => {
                            input_closed = true;
                            transformer.handle_input_close().unwrap();
                        }
                    }

                }

                Ok(())
            });

            res
        }
    };
}

pub async fn route_source<S>(
    mut packet_tx: postage::broadcast::Sender<Arc<FFMpegPacket>>,
    mut source: S,
    streams: impl IntoIterator<Item = Box<dyn FnMut() -> FFMPEGResult<()> + Send>>,
) -> Vec<Result<FFMPEGResult<()>, JoinError>>
where
    S: PullSource<Err = FFMPEGError, Output = FFMpegPacket> + Send + 'static,
{
    let mut handles = Vec::new();
    for s in streams {
        handles.push(tokio::task::spawn_blocking(s));
    }

    handles.push(tokio::task::spawn_blocking(move || -> FFMPEGResult<()> {
        while let Some(next_packet) = source.pull()? {
            packet_tx.blocking_send(Arc::new(next_packet));
        }

        drop(packet_tx);

        Ok(())
    }));

    futures::future::join_all(handles).await
}
