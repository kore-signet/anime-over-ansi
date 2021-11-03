use anime_telnet::{encoding::*, metadata::*};
use clap::Arg;
use opencv::core::{Mat, MatTraitConst, Vector};
use opencv::videoio::{
    VideoCapture, VideoCaptureProperties, VideoCaptureTrait, VideoCaptureTraitConst,
};

use image::{buffer::ConvertBuffer, Bgr, ImageBuffer, RgbaImage};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, prelude::*, BufWriter};
use std::path::Path;
use std::time::SystemTime;

use fast_image_resize as fr;

use tempfile::tempfile;

// gets and removes value from hashmap for whichever one of the keys exists in it
fn one_of_keys(map: &mut HashMap<String, String>, keys: Vec<&'static str>) -> Option<String> {
    for k in keys {
        if let Some(v) = map.remove(k) {
            return Some(v);
        }
    }

    None
}

fn main() -> std::io::Result<()> {
    let matches = clap::App::new("anime over telnet encoder")
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

    let input_file = matches.value_of("INPUT").unwrap();
    let out_fs = File::create(matches.value_of("OUT").unwrap()).unwrap();
    let mut out_writer = BufWriter::new(out_fs);

    let mut video_cap = VideoCapture::from_file(input_file, 0).expect("couldn't open video file");
    let opencv_fps = video_cap
        .get(VideoCaptureProperties::CAP_PROP_FPS as i32)
        .ok();

    let mut video_tracks: Vec<(Encoder, VideoTrack)> = matches
        .values_of("track")
        .unwrap()
        .map(|cfg| {
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

            (
                Encoder {
                    needs_width: width,
                    needs_height: height,
                    needs_color: color_mode,
                    frame_lengths: Vec::new(),
                    output: {
                        let file = tempfile().unwrap();
                        if compression == CompressionMode::Zstd {
                            OutputStream::CompressedFile({
                                let mut encoder = zstd::Encoder::new(file, 3).unwrap(); // todo: configurable compression level
                                encoder.long_distance_matching(true).unwrap();
                                encoder
                            })
                        } else {
                            OutputStream::File(file)
                        }
                    },
                },
                VideoTrack {
                    name: one_of_keys(&mut map, vec!["title", "n", "name"]),
                    color_mode: color_mode,
                    height: height,
                    width: width,
                    compression: compression,
                    encode_time: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    framerate: one_of_keys(&mut map, vec!["fps", "rate", "framerate", "r"])
                        .map(|v| v.parse::<f64>().unwrap())
                        .unwrap_or_else(|| opencv_fps.unwrap()),
                    offset: 0,
                    length: 0,
                    frame_lengths: Vec::new(),
                },
            )
        })
        .collect();

    let subtitle_tracks: Vec<(File, SubtitleTrack)> = matches
        .values_of("subtitle_track")
        .unwrap()
        .map(|cfg| {
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
            let mut sfile = File::open(&file_name).unwrap();
            let s_len = sfile.metadata().unwrap().len();
            let mut contents: Vec<u8> = Vec::with_capacity(s_len as usize);
            sfile.read_to_end(&mut contents).unwrap();
            sfile.rewind().unwrap();

            let format =
                subparse::get_subtitle_format(Path::new(&file_name).extension(), &contents)
                    .expect("couldn't guess subtitle format");
            (
                sfile,
                SubtitleTrack {
                    name: one_of_keys(&mut map, vec!["name", "n", "title"]),
                    lang: one_of_keys(&mut map, vec!["lang"]),
                    format: format,
                    offset: 0,
                    length: s_len,
                },
            )
        })
        .collect();

    let resize_filter = match matches.value_of("resize").unwrap_or("hamming") {
        "nearest" => fr::FilterType::Box,
        "bilinear" => fr::FilterType::Bilinear,
        "hamming" => fr::FilterType::Hamming,
        "catmullrom" => fr::FilterType::CatmullRom,
        "mitchell" => fr::FilterType::Mitchell,
        "lanczos" => fr::FilterType::Lanczos3,
        _ => fr::FilterType::Hamming,
    };

    let frame_quant = video_cap
        .get(VideoCaptureProperties::CAP_PROP_FRAME_COUNT as i32)
        .unwrap();

    let video_width = video_cap
        .get(VideoCaptureProperties::CAP_PROP_FRAME_WIDTH as i32)
        .unwrap() as i32;

    let video_height = video_cap
        .get(VideoCaptureProperties::CAP_PROP_FRAME_HEIGHT as i32)
        .unwrap() as i32;

    let mut processor_pipeline: HashMap<(u32, u32), ProcessorPipeline> = HashMap::new();

    for (e, _) in video_tracks.iter() {
        processor_pipeline
            .entry((e.needs_width, e.needs_height))
            .or_insert(ProcessorPipeline {
                width: e.needs_width,
                height: e.needs_height,
                filter: resize_filter,
                color_modes: HashSet::new(),
                last_frames: HashMap::new(),
            })
            .color_modes
            .insert(e.needs_color);
    }

    let mut mat = Mat::default();

    let encoder_bar = ProgressBar::new(frame_quant as u64);
    encoder_bar.set_style(
        ProgressStyle::default_bar()
            .template("{per_sec:5!}fps - {percent}% done, encoding frame {pos} out of {len}\n{bar:40.green/white}"),
    );

    let mut buffer = Vector::<u8>::with_capacity((video_width * video_height * 3) as usize);

    loop {
        let read_frame = video_cap.read(&mut mat).unwrap();
        if !read_frame {
            break;
        }

        mat.reshape(1, 1).unwrap().copy_to(&mut buffer).unwrap();

        let img: RgbaImage = ImageBuffer::<Bgr<u8>, Vec<u8>>::from_raw(
            video_width as u32,
            video_height as u32,
            buffer.to_vec(),
        )
        .unwrap()
        .convert();

        for pipeline in processor_pipeline.values_mut() {
            pipeline.process(&img);
        }

        for (encoder, _) in video_tracks.iter_mut() {
            encoder
                .encode_frame(
                    processor_pipeline[&(encoder.needs_width, encoder.needs_height)]
                        .last_frame(&encoder.needs_color),
                )
                .unwrap();
        }

        encoder_bar.inc(1);
    }

    encoder_bar.finish();

    let mut track_position = 0;
    let mut track_files: Vec<File> = Vec::new();
    let mut done_tracks: Vec<VideoTrack> = Vec::new();
    let mut done_subtitles: Vec<SubtitleTrack> = Vec::new();

    for (encoder, mut track) in video_tracks {
        let (frame_lengths, mut file) = encoder.finish().unwrap();
        file.flush().unwrap();

        track.frame_lengths = frame_lengths;
        track.offset = track_position;
        track.length = file.metadata().unwrap().len();

        track_position += track.length;

        file.rewind().unwrap();

        track_files.push(file);
        done_tracks.push(track);
    }

    for (file, mut track) in subtitle_tracks {
        track.offset = track_position;
        track_position += track.length;

        track_files.push(file);
        done_subtitles.push(track)
    }

    let metadata_bytes = serde_json::to_vec(&VideoMetadata {
        video_tracks: done_tracks,
        subtitle_tracks: done_subtitles,
    })
    .unwrap();
    out_writer
        .write_all(&(metadata_bytes.len() as u64).to_be_bytes())
        .unwrap();
    out_writer.write_all(&metadata_bytes).unwrap();

    for mut file in track_files {
        io::copy(&mut file, &mut out_writer).unwrap();
    }

    Ok(())
}
