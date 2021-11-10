use anime_telnet::{encoding::*, metadata::*};

use crossbeam::channel::{bounded, Receiver, Sender};
use image::{RgbImage, RgbaImage};
use indicatif::{ProgressBar, ProgressStyle};

use ac_ffmpeg::codec::video::{PixelFormat, VideoDecoder, VideoFrameScaler};
use ac_ffmpeg::codec::Decoder;
use ac_ffmpeg::format::demuxer::Demuxer;

use std::str::FromStr;

pub fn read_video(
    video_decoder: &mut VideoDecoder,
    demuxer: &mut Demuxer<std::fs::File>,
    stream_index: usize,
    snd_img: Sender<RgbaImage>,
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
            continue;
        }

        video_decoder.push(packet).unwrap();

        while let Some(frame) = video_decoder.take().unwrap() {
            let frame = color_transformer.scale(&frame).unwrap();
            snd_img
                .send(
                    RgbaImage::from_raw(
                        codec_parameters.width() as u32,
                        codec_parameters.height() as u32,
                        frame.planes()[0].data().to_vec(),
                    )
                    .unwrap(),
                )
                .unwrap();
        }
    }

    video_decoder.flush().unwrap();

    while let Some(frame) = video_decoder.take().unwrap() {
        let frame = color_transformer.scale(&frame).unwrap();
        snd_img
            .send(
                RgbaImage::from_raw(
                    codec_parameters.width() as u32,
                    codec_parameters.height() as u32,
                    frame.planes()[0].data().to_vec(),
                )
                .unwrap(),
            )
            .unwrap();
    }
}

pub fn resize_and_dither(
    pipelines: &Vec<ProcessorPipeline>,
    rcv_img: Receiver<RgbaImage>,
    snd_resized: Sender<Vec<(u32, u32, ColorMode, RgbImage)>>,
) {
    for img in rcv_img.iter() {
        snd_resized
            .send(
                pipelines
                    .iter()
                    .flat_map(|p| {
                        p.process(&img)
                            .into_iter()
                            .map(move |r| (p.width, p.height, r.0, r.1))
                    })
                    .collect::<Vec<(u32, u32, ColorMode, RgbImage)>>(),
            )
            .unwrap();
    }
}

pub fn encode(
    video_decoder: &mut VideoDecoder,
    demuxer: &mut Demuxer<std::fs::File>,
    stream_index: usize,
    pipelines: &Vec<ProcessorPipeline>,
    tracks: &mut Vec<(Encoder, VideoTrack)>,
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
        s.spawn(|_| read_video(video_decoder, demuxer, stream_index, snd_img));

        s.spawn(|_| resize_and_dither(pipelines, rcv_img, snd_resized));

        let mut idx = 0;

        for msg in rcv_resized.iter() {
            for (encoder, _) in tracks.iter_mut() {
                encoder
                    .encode_frame(
                        &msg[msg
                            .iter()
                            .position(|v| {
                                v.0 == encoder.needs_width
                                    && v.1 == encoder.needs_height
                                    && v.2 == encoder.needs_color
                            })
                            .unwrap()]
                        .3,
                    )
                    .unwrap();
            }

            idx += 1;
            encoder_bar.set_position(idx);
        }

        encoder_bar.finish_at_current_pos();
        println!("finished encoding; writing final file..");
    })
    .unwrap();

    Ok(())
}
