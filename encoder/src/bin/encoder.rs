use anime_telnet::{encoding::*, metadata::*};
use anime_telnet_encoder::{ANSIVideoEncoder, PacketCodec};
use clap::Arg;

use cyanotype::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;

use std::time::{SystemTime};
use tokio_util::codec::FramedWrite;

use fast_image_resize as fr;

use futures::sink::{SinkExt};
use futures::stream::{StreamExt};
use image::RgbImage;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::{AsyncWriteExt};

use rmp_serde as rmps;

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

    ac_ffmpeg::set_log_callback(|_, m| {
        println!("ffmpeg: {}", m);
    });

    let input_file = File::open(matches.value_of("INPUT").unwrap()).unwrap();
    let mut demuxer = Demuxer::from_seek(input_file).unwrap();

    let _out_fs = File::create(matches.value_of("OUT").unwrap()).unwrap();

    let mut track_index: i32 = -1;

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
                ANSIVideoEncoder {
                    stream_index: track_index as u32,
                    width: width,
                    height: height,
                    color_mode: color_mode,
                    encoder_opts: EncoderOptions {
                        compression_level: Some(compression_level.unwrap_or(3)),
                        compression_mode: compression,
                    },
                },
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
        let mut encoders: Vec<ANSIVideoEncoder> = Vec::new();
        let theme = dialoguer::theme::ColorfulTheme::default();

        loop {
            let add_track = dialoguer::Select::with_theme(&theme)
                .with_prompt("add a video track?")
                .items(&["yes!", "finish video track configuration"])
                .interact()
                .unwrap();
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

                encoders.push(ANSIVideoEncoder {
                    stream_index: track_index as u32,
                    width: width,
                    height: height,
                    color_mode: color_mode,
                    encoder_opts: EncoderOptions {
                        compression_level: Some(3),
                        compression_mode: compression,
                    },
                });

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

    let resize_filter = match matches.value_of("resize").unwrap_or("hamming") {
        "nearest" => fr::FilterType::Box,
        "bilinear" => fr::FilterType::Bilinear,
        "hamming" => fr::FilterType::Hamming,
        "catmullrom" => fr::FilterType::CatmullRom,
        "mitchell" => fr::FilterType::Mitchell,
        "lanczos" => fr::FilterType::Lanczos3,
        _ => fr::FilterType::Hamming,
    };

    let mut out_file = tokio::fs::File::create(matches.value_of("OUT").unwrap()).await?;
    let metadata_bytes = rmps::to_vec(&VideoMetadata {
        video_tracks,
        subtitle_tracks: vec![],
    })
    .unwrap();

    out_file.write_u64(metadata_bytes.len() as u64).await?;
    out_file.write_all(&metadata_bytes).await?;

    let mut processor_pipeline: HashMap<(u32, u32), ProcessorPipeline> = HashMap::new();

    for e in encoders.iter() {
        processor_pipeline
            .entry((e.width, e.height))
            .or_insert(ProcessorPipeline {
                width: e.width,
                height: e.height,
                filter: resize_filter,
                color_modes: HashSet::new(),
            })
            .color_modes
            .insert(e.color_mode);
    }

    let video_stream = demuxer.subscribe_to_video(0).unwrap();

    let demuxer_task = tokio::task::spawn(async move {
        demuxer.run().await.unwrap();
    });

    let pipelines: Vec<ProcessorPipeline> =
        processor_pipeline.into_iter().map(|(_,v)| v).collect();

    let encoder_bar = ProgressBar::new_spinner();
    encoder_bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner} {per_sec:5!}fps - encoding frame {pos}"),
    );

    encoder_bar.enable_steady_tick(200);

    let resized_stream = video_stream
        .enumerate()
        .map(|(i, img)| {
            let time = img.time;
            (
                i,
                pipelines
                    .iter()
                    .flat_map(move |p| {
                        p.process(&img.frame).into_iter().map(move |r| {
                            ((p.width, p.height, r.0), VideoPacket { frame: r.1, time })
                        })
                    })
                    .collect::<HashMap<(u32, u32, ColorMode), VideoPacket<RgbImage>>>(),
            )
        })
        .flat_map(|(i, frames)| {
            encoder_bar.set_position(i as u64);

            futures::stream::iter(encoders.iter().map(move |encoder| {
                Ok(encoder
                    .encode_frame(&frames[&(encoder.width, encoder.height, encoder.color_mode)]))
            }))
        });

    let out_stream = FramedWrite::new(out_file, PacketCodec::new()).buffer(256);

    resized_stream.forward(out_stream).await?;

    demuxer_task.await?;

    encoder_bar.finish_at_current_pos();

    // .split();
    // .for_each(|_| futures::future::ready(()))
    // .await;

    // .map(|(width, height, color_mode, packet)| {

    //     encoders
    //         .iter()
    //         .filter(|e| {
    //             e.width == width && e.height == height && e.color_mode == color_mode
    //         })
    //         .map(|encoder| encoder.encode_frame(&packet))
    // });

    let _idx = 0;

    // let subtitle_tracks: Vec<(File, SubtitleTrack)> =
    //     if let Some(vals) = matches.values_of("subtitle_track") {
    //         vals.map(|cfg| {
    //             let mut map: HashMap<String, String> = cfg
    //                 .split_terminator(',')
    //                 .map(|line| {
    //                     line.split_once(':')
    //                         .map(|(k, v)| (k.to_owned(), v.to_owned()))
    //                         .unwrap()
    //                 })
    //                 .collect();

    //             let file_name = one_of_keys(&mut map, vec!["source", "file", "f"])
    //                 .expect("please specify a subtitle file!");
    //             let mut sfile = File::open(&file_name).unwrap();
    //             let s_len = sfile.metadata().unwrap().len();
    //             let mut contents: Vec<u8> = Vec::with_capacity(s_len as usize);
    //             sfile.read_to_end(&mut contents).unwrap();
    //             sfile.rewind().unwrap();

    //             let format =
    //                 subparse::get_subtitle_format(Path::new(&file_name).extension(), &contents)
    //                     .expect("couldn't guess subtitle format");
    //             (
    //                 sfile,
    //                 SubtitleTrackBuilder::default()
    //                     .name(one_of_keys(&mut map, vec!["name", "n", "title"]))
    //                     .lang(one_of_keys(&mut map, vec!["lang"]))
    //                     .format(format)
    //                     .length(s_len)
    //                     .build()
    //                     .unwrap(),
    //             )
    //         })
    //         .collect()
    //     } else {
    //         let mut subtitle_tracks: Vec<(File, SubtitleTrack)> = Vec::new();
    //         let theme = dialoguer::theme::ColorfulTheme::default();

    //         loop {
    //             let add_track = dialoguer::Select::with_theme(&theme)
    //                 .with_prompt("add a subtitle track?")
    //                 .items(&["yes!", "finish subtitle track configuration"])
    //                 .interact()
    //                 .unwrap();
    //             if add_track == 1 {
    //                 break;
    //             } else {
    //                 let track_name: String = dialoguer::Input::with_theme(&theme)
    //                     .with_prompt("track name")
    //                     .interact_text()
    //                     .unwrap();
    //                 let track_lang: String = dialoguer::Input::with_theme(&theme)
    //                     .with_prompt("track language")
    //                     .interact_text()
    //                     .unwrap();
    //                 let file_name: String = dialoguer::Input::with_theme(&theme)
    //                     .with_prompt("subtitle file")
    //                     .interact_text()
    //                     .unwrap();
    //                 let mut sfile = File::open(&file_name).unwrap();
    //                 let s_len = sfile.metadata().unwrap().len();
    //                 let mut contents: Vec<u8> = Vec::with_capacity(s_len as usize);
    //                 sfile.read_to_end(&mut contents).unwrap();
    //                 sfile.rewind().unwrap();

    //                 let format =
    //                     subparse::get_subtitle_format(Path::new(&file_name).extension(), &contents)
    //                         .expect("couldn't guess subtitle format");
    //                 subtitle_tracks.push((
    //                     sfile,
    //                     SubtitleTrackBuilder::default()
    //                         .name(Some(track_name))
    //                         .lang(Some(track_lang))
    //                         .format(format)
    //                         .length(s_len)
    //                         .build()
    //                         .unwrap(),
    //                 ));
    //             }
    //         }
    //         subtitle_tracks
    //     };

    // while let Some(f) = resized_stream.next().await {

    // }

    // println!("finished encoding; writing finished file..");

    // let mut track_position = 0;
    // let mut track_files: Vec<File> = Vec::new();
    // let mut done_tracks: Vec<VideoTrack> = Vec::new();
    // let mut done_subtitles: Vec<SubtitleTrack> = Vec::new();

    // for (encoder, mut track) in video_tracks {
    //     let (frame_lengths, frame_hashes, frame_times, mut file) = encoder.finish().unwrap();
    //     file.flush().unwrap();

    //     track.frame_times = frame_times;
    //     track.frame_hashes = frame_hashes;
    //     track.frame_lengths = frame_lengths;
    //     track.offset = track_position;
    //     track.length = file.metadata().unwrap().len();

    //     track_position += track.length;

    //     file.rewind().unwrap();

    //     track_files.push(file);
    //     done_tracks.push(track);
    // }

    // for (file, mut track) in subtitle_tracks {
    //     track.offset = track_position;
    //     track_position += track.length;

    //     track_files.push(file);
    //     done_subtitles.push(track)
    // }

    // let metadata_bytes = serde_json::to_vec(&VideoMetadata {
    //     video_tracks: done_tracks,
    //     subtitle_tracks: done_subtitles,
    // })
    // .unwrap();
    // out_writer
    //     .write_all(&(metadata_bytes.len() as u64).to_be_bytes())
    //     .unwrap();
    // out_writer.write_all(&metadata_bytes).unwrap();

    // for mut file in track_files {
    //     io::copy(&mut file, &mut out_writer).unwrap();
    // }

    // Ok(())
    Ok(())
}
