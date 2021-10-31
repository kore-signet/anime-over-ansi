use anime_telnet::{encoding::*, metadata::*, palette::LABAnsiColorMap};
use clap::Arg;
use image::imageops;
use opencv::core::{Mat, Vector};
use opencv::videoio::{VideoCapture, VideoCaptureProperties, VideoCaptureTrait};

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, prelude::*, BufWriter};
use std::time::SystemTime;
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
                .possible_values(&["nearest", "triangle", "gaussian", "lanczos"]),
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

    let resize_filter = match matches.value_of("resize").unwrap_or("triangle") {
        "nearest" => imageops::FilterType::Nearest,
        "triangle" => imageops::FilterType::Triangle,
        "gaussian" => imageops::FilterType::Gaussian,
        "lanczos" => imageops::FilterType::Lanczos3,
        _ => imageops::FilterType::Triangle,
    };

    let frame_quant = video_cap
        .get(VideoCaptureProperties::CAP_PROP_FRAME_COUNT as i32)
        .unwrap();

    let mut resize_pipelines: HashMap<(u32, u32), ResizePipeline> = video_tracks
        .iter()
        .map(|(e, _)| {
            (
                (e.needs_width, e.needs_height),
                ResizePipeline {
                    width: e.needs_width,
                    height: e.needs_height,
                    filter: resize_filter,
                    last_frame: None,
                },
            )
        })
        .collect();

    let cursor_pos = 2;

    let mut mat = Mat::default();
    let mut buffer: Vector<u8> = Vector::new();
    let mut i: u64 = 0;

    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush().unwrap();

    loop {
        let percent_done = ((i + 1) as f64 / frame_quant as f64) * 100.0;

        print!("\x1B[{};1H", cursor_pos);
        print!("\x1B[2K");
        print!(
            "{:.2}% | Processing frame #{}/{} | reading, resizing",
            percent_done,
            i + 1,
            frame_quant
        );
        io::stdout().flush().unwrap();

        let read_frame = video_cap.read(&mut mat).unwrap();
        if !read_frame {
            break;
        }

        opencv::imgcodecs::imencode(".png", &mat, &mut buffer, &Vector::new()).unwrap();
        let img = image::load_from_memory_with_format(&buffer.to_vec(), image::ImageFormat::Png)
            .expect("couldn't load image")
            .into_rgb8();

        for pipeline in resize_pipelines.values_mut() {
            pipeline.resize(&img);
        }

        print!("\x1B[{};1H", cursor_pos);
        print!("\x1B[2K");
        print!(
            "{:.2}% | Processing frame #{}/{} | dithering",
            percent_done,
            i + 1,
            frame_quant
        );
        io::stdout().flush().unwrap();

        for (encoder, _) in video_tracks.iter_mut() {
            if encoder.needs_color == ColorMode::EightBit {
                let mut frame = resize_pipelines[&(encoder.needs_width, encoder.needs_height)]
                    .last_frame()
                    .clone();
                imageops::dither(&mut frame, &LABAnsiColorMap);
                encoder.encode_frame(&frame).unwrap();
            } else {
                encoder
                    .encode_frame(
                        resize_pipelines[&(encoder.needs_width, encoder.needs_height)].last_frame(),
                    )
                    .unwrap();
            }
        }

        i += 1;
    }

    let mut track_position = 0;
    let mut track_files: Vec<File> = Vec::new();
    let mut done_tracks: Vec<VideoTrack> = Vec::new();

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

    let metadata_bytes = serde_json::to_vec(&done_tracks).unwrap();
    out_writer
        .write_all(&(metadata_bytes.len() as u64).to_be_bytes())
        .unwrap();
    out_writer.write_all(&metadata_bytes).unwrap();

    for mut file in track_files {
        io::copy(&mut file, &mut out_writer).unwrap();
    }

    Ok(())
}
