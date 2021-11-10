use anime_telnet::{encoding::*, metadata::*};

use crossbeam::channel::{bounded, Receiver, Sender};
use image::{RgbImage, RgbaImage};
use indicatif::{ProgressBar, ProgressStyle};

use ac_ffmpeg::codec::video::{PixelFormat, VideoDecoder, VideoFrameScaler};
use ac_ffmpeg::codec::Decoder;
use ac_ffmpeg::format::demuxer::Demuxer;
use ac_ffmpeg::packet::Packet;

use std::str::FromStr;

pub struct ImageFrame<T> {
    time: i64, // nanoseconds
    image: T,
}

pub fn read_video(
    video_decoder: &mut VideoDecoder,
    demuxer: &mut Demuxer<std::fs::File>,
    stream_index: usize,
    packet_channel: Option<Sender<Packet>>, // optional channel to send extra packets to. should probably be unbounded, considering it'll block all video reading if full.
    snd_img: Sender<ImageFrame<RgbaImage>>,
) {
    let codec_parameters = video_decoder.codec_parameters();
    let mut color_transformer = VideoFrameScaler::builder()
        .source_pixel_format(codec_parameters.pixel_format())
        .source_height(codec_parameters.height())
        .source_width(codec_parameters.width())
        .target_height(codec_parameters.height())
        .target_width(codec_parameters.width())
        .target_pixel_format(PixelFormat::from_str("rgba").unwrap())
        .build()
        .unwrap();

    while let Some(packet) = demuxer.take().unwrap() {
        if packet.stream_index() != stream_index {
            if let Some(ref tx) = packet_channel {
                tx.send(packet).unwrap();
            }

            continue;
        }

        video_decoder.push(packet).unwrap();

        while let Some(frame) = video_decoder.take().unwrap() {
            let frame = color_transformer.scale(&frame).unwrap();
            snd_img
                .send(ImageFrame {
                    time: frame.pts().as_nanos().unwrap(),
                    image: RgbaImage::from_raw(
                        codec_parameters.width() as u32,
                        codec_parameters.height() as u32,
                        frame.planes()[0].data().to_vec(),
                    )
                    .unwrap(),
                })
                .unwrap();
        }
    }

    video_decoder.flush().unwrap();

    while let Some(frame) = video_decoder.take().unwrap() {
        let frame = color_transformer.scale(&frame).unwrap();
        snd_img
            .send(ImageFrame {
                time: frame.pts().as_nanos().unwrap(),
                image: RgbaImage::from_raw(
                    codec_parameters.width() as u32,
                    codec_parameters.height() as u32,
                    frame.planes()[0].data().to_vec(),
                )
                .unwrap(),
            })
            .unwrap();
    }
}

pub fn resize_and_dither(
    pipelines: &Vec<ProcessorPipeline>,
    rcv_img: Receiver<ImageFrame<RgbaImage>>,
    snd_resized: Sender<Vec<(u32, u32, ColorMode, ImageFrame<RgbImage>)>>,
) {
    for img in rcv_img.iter() {
        let time = img.time;
        snd_resized
            .send(
                pipelines
                    .iter()
                    .flat_map(|p| {
                        p.process(&img.image).into_iter().map(move |r| {
                            (
                                p.width,
                                p.height,
                                r.0,
                                ImageFrame {
                                    image: r.1,
                                    time,
                                },
                            )
                        })
                    })
                    .collect::<Vec<(u32, u32, ColorMode, ImageFrame<RgbImage>)>>(),
            )
            .unwrap();
    }
}

pub fn encode_to_files(
    video_decoder: &mut VideoDecoder,
    demuxer: &mut Demuxer<std::fs::File>,
    stream_index: usize,
    pipelines: &Vec<ProcessorPipeline>,
    tracks: &mut Vec<(FileEncoder, VideoTrack)>,
    show_progress: bool,
) -> std::io::Result<()> {
    let encoder_bar = ProgressBar::new_spinner();
    if !show_progress {
        encoder_bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    } else {
        encoder_bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner} {per_sec:5!}fps - encoding frame {pos}"),
        );
        encoder_bar.enable_steady_tick(200);
    }

    let (snd_img, rcv_img) = bounded(64);
    let (snd_resized, rcv_resized) = bounded(64);

    crossbeam::scope(|s| {
        s.spawn(|_| read_video(video_decoder, demuxer, stream_index, None, snd_img));

        s.spawn(|_| resize_and_dither(pipelines, rcv_img, snd_resized));

        let mut idx = 0;

        for msg in rcv_resized.iter() {
            for (encoder, _) in tracks.iter_mut() {
                let frame_index = msg
                    .iter()
                    .position(|v| {
                        v.0 == encoder.needs_width
                            && v.1 == encoder.needs_height
                            && v.2 == encoder.needs_color
                    })
                    .unwrap();
                encoder
                    .write_frame(&msg[frame_index].3.image, msg[frame_index].3.time)
                    .unwrap();
            }

            idx += 1;
            encoder_bar.set_position(idx);
        }

        encoder_bar.finish_at_current_pos();
        println!("finished encoding; writing finished file..");
    })
    .unwrap();

    Ok(())
}
