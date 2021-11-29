use anime_telnet::{encoding::*, metadata::*};
use anime_telnet_encoder::{
    subtitles, ANSIVideoEncoder, PacketWriteCodec as PacketCodec, SpinnyANSIVideoEncoder,
};
use clap::Arg;

use cyanotype::*;
use std::collections::{HashMap, HashSet};

use indicatif::{MultiProgress, ProgressDrawTarget};
use std::time::SystemTime;
use tokio_util::codec::FramedWrite;

use fast_image_resize as fr;

use futures::sink::SinkExt;
use futures::stream::{Stream, StreamExt};
use image::RgbImage;
use rmp_serde as rmps;
use std::path::Path;
use std::pin::Pin;
use tokio::io::AsyncWriteExt;

// gets and removes value from hashmap for whichever one of the keys exists in it
fn one_of_keys(map: &mut HashMap<String, String>, keys: Vec<&'static str>) -> Option<String> {
    for k in keys {
        if let Some(v) = map.remove(k) {
            return Some(v);
        }
    }

    None
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("ansi.moe encoder")
        .version("1.0")
        .author("allie signet <allie@cat-girl.gay>")
        .about("encodes video into ANSI escape sequences")
        .arg(
            Arg::with_name("INPUT")
                .help("file to read from")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("OUT")
                .help("file to write output to")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::with_name("track")
                .short("t")
                .long("track")
                .takes_value(true)
                .multiple(true)
                .number_of_values(1)
                .validator(|s| {
                    for c in s.split_terminator(',') {
                        if let Some((k, _v)) = c.split_once(':') {
                            if [
                                "width",
                                "w",
                                "height",
                                "h",
                                "color",
                                "color_mode",
                                "c",
                                "fps",
                                "rate",
                                "framerate",
                                "r",
                                "name",
                                "title",
                                "n",
                                "compression",
                                "zstd-level",
                                "compression-level",
                            ]
                            .contains(&k)
                            {
                                continue;
                            } else {
                                return Err("invalid configuration key for track".to_owned());
                            }
                        } else {
                            return Err("invalid configuration format - use key:value".to_owned());
                        }
                    }

                    Ok(())
                }),
        )
        .arg(
            Arg::with_name("subtitle_track")
                .short("s")
                .long("subtitles")
                .takes_value(true)
                .multiple(true)
                .validator(|s| {
                    for _c in s.split_terminator(',') {
                        if let Some((k, _v)) = s.split_once(':') {
                            if ["name", "n", "title", "lang", "file", "source", "f"].contains(&k) {
                                continue;
                            } else {
                                return Err("invalid configuration key for track".to_owned());
                            }
                        } else {
                            return Err("invalid configuration format - use key:value".to_owned());
                        }
                    }

                    Ok(())
                }),
        )
        .arg(
            Arg::with_name("resizing")
                .help("resizing algorithm to use")
                .long("resize")
                .takes_value(true)
                .possible_values(&[
                    "nearest",
                    "hamming",
                    "catmullrom",
                    "mitchell",
                    "lanczos",
                    "bilinear",
                ]),
        )
        .get_matches();

    let mut demuxer = Demuxer::from_url(matches.value_of("INPUT").unwrap()).unwrap();
    demuxer.block_video_streams(true);

    let mut track_index: i32 = -1;

    let progress_bars = MultiProgress::new();
    progress_bars.set_draw_target(ProgressDrawTarget::hidden());

    let (encoders, video_tracks) = if let Some(vals) = matches.values_of("track") {
        vals.map(|cfg| {
            let mut map: HashMap<String, String> = cfg
                .split_terminator(',')
                .map(|line| {
                    line.split_once(':')
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                        .unwrap()
                })
                .collect();

            let color_mode = one_of_keys(&mut map, vec!["c", "color", "color_mode"])
                .map(|c| match c.as_str() {
                    "256" | "256color" => ColorMode::EightBit,
                    "true" | "truecolor" => ColorMode::True,
                    _ => panic!("invalid color mode: possible ones are '256' and 'true'"),
                })
                .unwrap_or(ColorMode::EightBit);

            let dither_mode = if color_mode == ColorMode::EightBit {
                one_of_keys(&mut map, vec!["dither", "dithering", "dithering-mode"])
                    .map(|c| match c.as_str() {
                        "floyd-steinberg" | "error-diffusion" => DitherMode::FloydSteinberg,
                        "ordered-2x2" => DitherMode::Pattern(2),
                        "ordered-4x4" => DitherMode::Pattern(4),
                        "ordered-8x8" => DitherMode::Pattern(8),
                        _ => panic!("invalid dithering mode: possible ones are 'floyd-steinberg', 'ordered-2x2', 'ordered-4x4', 'ordered-8x8'")
                    })
                    .unwrap_or(DitherMode::FloydSteinberg)
            } else {
                DitherMode::None
            };

            let height = one_of_keys(&mut map, vec!["h", "height"])
                .map(|h| h.parse::<u32>().expect("invalid number for height"))
                .unwrap_or(108);

            let width = one_of_keys(&mut map, vec!["w", "width"])
                .map(|h| h.parse::<u32>().expect("invalid number for width"))
                .unwrap_or(192);

            let compression_level = one_of_keys(&mut map, vec!["compression-level", "zstd-level"])
                .and_then(|v| v.parse::<i32>().ok());

            let compression = map
                .remove("compression")
                .map(|c| match c.as_str() {
                    "zstd" | "zstandard" => CompressionMode::Zstd,
                    "none" | "no" => CompressionMode::None,
                    _ => {
                        panic!("invalid compression mode: possible ones are 'zstd' and 'none'")
                    }
                })
                .unwrap_or(CompressionMode::None);

            track_index += 1;

            (
                SpinnyANSIVideoEncoder::from_underlying(
                    ANSIVideoEncoder {
                        stream_index: track_index as u32,
                        width,
                        height,
                        color_mode,
                        dither_mode,
                        encoder_opts: EncoderOptions {
                            compression_level: Some(compression_level.unwrap_or(3)),
                            compression_mode: compression,
                        },
                    },
                    &progress_bars,
                ),
                VideoTrackBuilder::default()
                    .name(one_of_keys(&mut map, vec!["title", "n", "name"]))
                    .color_mode(color_mode)
                    .height(height)
                    .width(width)
                    .compression(compression)
                    .index(track_index as u32)
                    .encode_time(
                        SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    )
                    .build()
                    .unwrap(),
            )
        })
        .unzip()
    } else {
        let mut video_tracks: Vec<VideoTrack> = Vec::new();
        let mut encoders: Vec<SpinnyANSIVideoEncoder> = Vec::new();
        let theme = dialoguer::theme::ColorfulTheme::default();

        loop {
            let add_track = if video_tracks.is_empty() {
                0
            } else {
                dialoguer::Select::with_theme(&theme)
                    .with_prompt("add another video track?")
                    .items(&["yes!", "finish video track configuration"])
                    .interact()
                    .unwrap()
            };

            if add_track == 1 {
                break;
            } else {
                let track_name: String = dialoguer::Input::with_theme(&theme)
                    .with_prompt("track name")
                    .interact_text()
                    .unwrap();
                let width = dialoguer::Input::with_theme(&theme)
                    .with_prompt("video width")
                    .default(192u32)
                    .interact_text()
                    .unwrap();
                let height = dialoguer::Input::with_theme(&theme)
                    .with_prompt("video height")
                    .default(108u32)
                    .interact_text()
                    .unwrap();
                let color_mode = match dialoguer::Select::with_theme(&theme)
                    .with_prompt("color mode")
                    .items(&["8bit", "full color"])
                    .interact()
                    .unwrap()
                {
                    0 => ColorMode::EightBit,
                    1 => ColorMode::True,
                    _ => panic!(),
                };

                let dither_mode = if color_mode == ColorMode::EightBit {
                    [
                        DitherMode::FloydSteinberg,
                        DitherMode::Pattern(2),
                        DitherMode::Pattern(4),
                        DitherMode::Pattern(8),
                    ][dialoguer::Select::with_theme(&theme)
                        .with_prompt("dithering mode")
                        .items(&[
                            "floyd-steinberg",
                            "ordered pattern dithering (2x2)",
                            "ordered pattern dithering (4x4)",
                            "ordered pattern dithering (8x8)",
                        ])
                        .interact()
                        .unwrap()]
                } else {
                    DitherMode::None
                };

                let compression = match dialoguer::Select::with_theme(&theme)
                    .with_prompt("compression mode")
                    .items(&["zstd", "none"])
                    .interact()
                    .unwrap()
                {
                    0 => CompressionMode::Zstd,
                    1 => CompressionMode::None,
                    _ => panic!(),
                };

                track_index += 1;

                encoders.push(SpinnyANSIVideoEncoder::from_underlying(
                    ANSIVideoEncoder {
                        stream_index: track_index as u32,
                        width,
                        height,
                        color_mode,
                        dither_mode,
                        encoder_opts: EncoderOptions {
                            compression_level: Some(3),
                            compression_mode: compression,
                        },
                    },
                    &progress_bars,
                ));

                video_tracks.push(
                    VideoTrackBuilder::default()
                        .name(Some(track_name))
                        .color_mode(color_mode)
                        .height(height)
                        .width(width)
                        .compression(compression)
                        .index(track_index as u32)
                        .encode_time(
                            SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        )
                        .build()
                        .unwrap(),
                );
            }
        }

        (encoders, video_tracks)
    };

    let (mut packet_streams, mut subtitle_tracks): (
        Vec<Pin<Box<dyn Stream<Item = std::io::Result<EncodedPacket>>>>>,
        Vec<SubtitleTrack>,
    ) = {
        demuxer
            .subtitle_streams
            .values()
            .map(|stream| {
                track_index += 1;
                let metadata = stream.metadata();
                let subtitle_track = SubtitleTrackBuilder::default()
                    .name(metadata.get("title").map(|v| v.to_string()))
                    .format(SubtitleFormat::from_codec_name(
                        stream.parameters().decoder_name().unwrap_or("undefined"),
                    ))
                    .codec_private(stream.extra_data().map(|v| v.to_owned()))
                    .index(track_index as u32)
                    .build()
                    .unwrap();

                let subtitle_encoder: Box<
                    dyn PacketTransformer<Source = cyanotype::SubtitlePacket>,
                > = match subtitle_track.format {
                    SubtitleFormat::SubRip => {
                        Box::new(subtitles::SRTEncoder::new(track_index as u32))
                    }
                    SubtitleFormat::SubStationAlpha => Box::new(subtitles::SSAEncoder::new(
                        track_index as u32,
                        vec![
                            "ReadOrder",
                            "Layer",
                            "Style",
                            "Name",
                            "MarginL",
                            "MarginR",
                            "MarginV",
                            "Effect",
                            "Text",
                        ]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                    )),
                    _ => Box::new(subtitles::PassthroughSubtitleEncoder::new(
                        track_index as u32,
                    )),
                };

                (
                    stream
                        .subscribe()
                        .filter_map(move |packet| {
                            futures::future::ready(subtitle_encoder.encode_packet(&packet))
                        })
                        .map(std::io::Result::Ok)
                        .boxed_local(),
                    subtitle_track,
                )
            })
            .unzip()
    };

    if let Some(vals) = matches.values_of("subtitle_track") {
        for cfg in vals {
            let mut map: HashMap<String, String> = cfg
                .split_terminator(',')
                .map(|line| {
                    line.split_once(':')
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                        .unwrap()
                })
                .collect();

            let file_name = one_of_keys(&mut map, vec!["source", "file", "f"])
                .expect("please specify a subtitle file!");
            let format = Path::new(&file_name)
                .extension()
                .and_then(|v| v.to_str())
                .and_then(|v| match v {
                    "srt" => Some(SubtitleFormat::SubRip),
                    "ass" | "ssa" => Some(SubtitleFormat::SubStationAlpha),
                    _ => None,
                })
                .expect("unknown subtitle format");

            let subf = tokio::fs::File::open(file_name).await?;

            track_index += 1;

            match format {
                SubtitleFormat::SubRip => {
                    let track = SubtitleTrackBuilder::default()
                        .name(one_of_keys(&mut map, vec!["name", "n", "title"]))
                        .lang(one_of_keys(&mut map, vec!["lang"]))
                        .format(format)
                        .index(track_index as u32)
                        .build()
                        .unwrap();

                    packet_streams
                        .push(subtitles::srt_file_to_packets(subf, track_index as u32).await?);
                    subtitle_tracks.push(track);
                }
                SubtitleFormat::SubStationAlpha => {
                    let track = SubtitleTrackBuilder::default()
                        .name(one_of_keys(&mut map, vec!["name", "n", "title"]))
                        .lang(one_of_keys(&mut map, vec!["lang"]))
                        .format(format)
                        .index(track_index as u32)
                        .build()
                        .unwrap();

                    let (stream, track) = subtitles::ssa_file_to_packets(subf, track).await?;
                    subtitle_tracks.push(track);
                    packet_streams.push(stream);
                }
                _ => unreachable!(),
            }
        }
    };

    let resize_filter = match matches.value_of("resize").unwrap_or("hamming") {
        "nearest" => fr::FilterType::Box,
        "bilinear" => fr::FilterType::Bilinear,
        "hamming" => fr::FilterType::Hamming,
        "catmullrom" => fr::FilterType::CatmullRom,
        "mitchell" => fr::FilterType::Mitchell,
        "lanczos" => fr::FilterType::Lanczos3,
        _ => fr::FilterType::Hamming,
    };

    progress_bars.set_draw_target(ProgressDrawTarget::stderr());

    let mut out_file = tokio::fs::File::create(matches.value_of("OUT").unwrap()).await?;
    let metadata_bytes = rmps::to_vec(&VideoMetadata {
        video_tracks,
        subtitle_tracks,
    })
    .unwrap();

    out_file.write_u64(metadata_bytes.len() as u64).await?;
    out_file.write_all(&metadata_bytes).await?;

    let mut processor_pipeline: HashMap<(u32, u32), ProcessorPipeline> = HashMap::new();

    for e in encoders.iter() {
        processor_pipeline
            .entry((e.underlying.width, e.underlying.height))
            .or_insert(ProcessorPipeline {
                width: e.underlying.width,
                height: e.underlying.height,
                filter: resize_filter,
                dither_modes: HashSet::new(),
            })
            .dither_modes
            .insert(e.underlying.dither_mode);
    }

    let video_stream = demuxer
        .subscribe_to_video(*demuxer.video_streams.keys().next().unwrap())
        .unwrap();

    demuxer.block_video_streams(false);
    let demuxer_task = tokio::task::spawn(async move {
        demuxer.run().await.unwrap();
    });

    let pipelines: Vec<ProcessorPipeline> =
        processor_pipeline.into_iter().map(|(_, v)| v).collect();

    let resized_stream = video_stream
        .map(move |img| {
            let time = img.time;

            pipelines
                .iter()
                .flat_map(move |p| {
                    p.process(&img.frame)
                        .into_iter()
                        .map(move |r| ((p.width, p.height, r.0), VideoPacket { frame: r.1, time }))
                })
                .collect::<HashMap<(u32, u32, DitherMode), VideoPacket<RgbImage>>>()
        })
        .flat_map(|frames| {
            futures::stream::iter(encoders.iter().map(move |encoder| {
                Ok(encoder
                    .encode_packet(
                        &frames[&(
                            encoder.underlying.width,
                            encoder.underlying.height,
                            encoder.underlying.dither_mode,
                        )],
                    )
                    .unwrap())
            }))
        });

    packet_streams.push(Box::pin(resized_stream));

    let out_stream = FramedWrite::new(out_file, PacketCodec::new()).buffer(256);

    futures::stream::select_all(packet_streams)
        .forward(out_stream)
        .await?;

    demuxer_task.await?;

    Ok(())
}
