use anime_telnet::{encoding::*, metadata::*, palette::PALETTE};
use clap::Arg;

use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};

use image::RgbImage;
use std::time::SystemTime;

use tempfile::tempfile;

pub fn is_eof<T>(err: &io::Result<T>) -> bool {
    match err {
        Ok(_) => false,
        Err(e) => e.kind() == io::ErrorKind::UnexpectedEof,
    }
}

// takes raw RGB24 or 8bit stream and muxes it into a container
fn main() -> std::io::Result<()> {
    let matches = clap::App::new("ansi.moe muxer")
        .version("1.0")
        .author("allie signet <allie@cat-girl.gay>")
        .about("takes raw byte stream and muxes it into container")
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
            Arg::with_name("width")
                .help("width to resize frames to (defaults to 192)")
                .short("w")
                .long("width")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("height")
                .help("height to resize frames to (defaults to 108)")
                .short("h")
                .long("height")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("color_mode")
                .help("ANSI color mode to use (defaults to 256color)")
                .short("c")
                .long("color")
                .takes_value(true)
                .possible_values(&["256color", "truecolor"]),
        )
        .arg(
            Arg::with_name("framerate")
                .help("framerate of video")
                .short("r")
                .long("fps")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("compression")
                .help("compression mode")
                .long("compress")
                .takes_value(true)
                .possible_values(&["none", "zstd"]),
        )
        .get_matches();

    let width = matches
        .value_of("width")
        .unwrap_or("192")
        .parse::<u32>()
        .expect("please specify a valid number for the width ><");
    let height = matches
        .value_of("height")
        .unwrap_or("108")
        .parse::<u32>()
        .expect("please specify a valid number for the height ><");
    let fps = matches
        .value_of("framerate")
        .unwrap_or("29.97")
        .parse::<f64>()
        .expect("please specify a valid number for the framerate ><");

    let color_mode = match matches.value_of("color_mode").unwrap_or("256color") {
        "truecolor" => ColorMode::True,
        "256color" => ColorMode::EightBit,
        _ => panic!(),
    };

    let compression_mode = match matches.value_of("compression").unwrap_or("none") {
        "none" => CompressionMode::None,
        "zstd" => CompressionMode::Zstd,
        _ => panic!(),
    };

    let mut encoder = Encoder {
        needs_width: width,
        needs_height: height,
        needs_color: color_mode,
        frame_lengths: Vec::new(),
        frame_hashes: Vec::new(),
        output: {
            let file = tempfile().unwrap();
            if compression_mode == CompressionMode::Zstd {
                OutputStream::CompressedFile({
                    let encoder = zstd::Encoder::new(file, 3).unwrap(); // todo, make configurable
                    encoder
                })
            } else {
                OutputStream::File(file)
            }
        },
    };

    let mut track = VideoTrack {
        name: None,
        color_mode,
        height,
        width,
        compression: compression_mode,
        encode_time: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        framerate: fps,
        offset: 0,
        length: 0,
        frame_lengths: Vec::new(),
        frame_hashes: Vec::new(),
    };

    let input_fs = File::open(matches.value_of("INPUT").unwrap()).unwrap();
    let mut input_r = BufReader::new(input_fs);

    let mut buffer: Vec<u8> = vec![0u8; width as usize * height as usize * color_mode.byte_size()];

    println!("reading frames!");

    loop {
        let read_res = input_r.read_exact(&mut buffer);
        if is_eof(&read_res) {
            break;
        } else {
            read_res.unwrap();
        }

        let img = if color_mode == ColorMode::True {
            RgbImage::from_raw(width, height, buffer.clone()).expect("couldn't read image")
        } else {
            let mut transformed_buffer: Vec<u8> =
                Vec::with_capacity(width as usize * height as usize * 3);
            for b in &buffer {
                let (r, g, b) = PALETTE[*b as usize];
                transformed_buffer.push(r);
                transformed_buffer.push(g);
                transformed_buffer.push(b);
            }
            RgbImage::from_raw(width, height, transformed_buffer).expect("couldn't read image")
        };

        encoder.encode_frame(&img).expect("couldn't write frame");
    }

    println!("done reading frames, encoding finished file");

    let out_fs = File::create(matches.value_of("OUT").unwrap()).unwrap();
    let mut out_writer = BufWriter::new(out_fs);

    let (frame_lengths, frame_hashes, mut file) = encoder.finish().unwrap();

    file.flush().unwrap();

    track.frame_hashes = frame_hashes;
    track.frame_lengths = frame_lengths;
    track.offset = 0;
    track.length = file.metadata().unwrap().len();

    file.rewind().unwrap();

    let metadata_bytes = serde_json::to_vec(&VideoMetadata {
        video_tracks: vec![track],
        subtitle_tracks: Vec::new(),
    })
    .unwrap();
    out_writer
        .write_all(&(metadata_bytes.len() as u64).to_be_bytes())
        .unwrap();
    out_writer.write_all(&metadata_bytes).unwrap();

    io::copy(&mut file, &mut out_writer).unwrap();

    Ok(())
}
