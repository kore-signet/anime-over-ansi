use anime_telnet::{encoding::*, metadata::*, palette::PALETTE};
use anime_telnet_encoder::{ANSIVideoEncoder, PacketWriteCodec as PacketCodec};
use clap::Arg;

use cyanotype::*;

use std::time::SystemTime;
use tokio_util::codec::FramedWrite;

use futures::sink::SinkExt;

use image::RgbImage;
use rmp_serde as rmps;

use std::time::Duration;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, BufReader};

pub fn is_eof<T>(err: &io::Result<T>) -> bool {
    match err {
        Ok(_) => false,
        Err(e) => e.kind() == io::ErrorKind::UnexpectedEof,
    }
}

// takes raw RGB24 or 8bit stream and muxes it into a container
#[tokio::main]
async fn main() -> std::io::Result<()> {
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

    let track = VideoTrack {
        name: None,
        color_mode,
        height,
        width,
        compression: compression_mode,
        encode_time: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        codec_private: None,
        index: 0,
    };

    let mut encoder = ANSIVideoEncoder {
        stream_index: 0,
        width,
        height,
        color_mode,
        dither_mode: DitherMode::None,
        diff: false,
        encoder_opts: PacketFlags {
            compression_level: Some(3),
            compression_mode,
            is_keyframe: true,
        },
        last_frame: None,
    };

    let mut out_file = tokio::fs::File::create(matches.value_of("OUT").unwrap()).await?;
    let metadata_bytes = rmps::to_vec(&VideoMetadata {
        attachments: Vec::new(),
        video_tracks: vec![track],
        subtitle_tracks: vec![],
    })
    .unwrap();

    out_file.write_u64(metadata_bytes.len() as u64).await?;
    out_file.write_all(&metadata_bytes).await?;
    let mut out_stream = FramedWrite::new(out_file, PacketCodec::new()).buffer(256);

    let input_fs = tokio::fs::File::open(matches.value_of("INPUT").unwrap())
        .await
        .unwrap();
    let mut input_r = BufReader::new(input_fs);

    let mut buffer: Vec<u8> = vec![0u8; width as usize * height as usize * color_mode.byte_size()];

    println!("reading frames!");

    let mut i: i64 = 0;
    let interval_nanos = (1000000000.0 / fps) as i64;

    loop {
        let read_res = input_r.read_exact(&mut buffer).await;
        if is_eof(&read_res) {
            break;
        } else {
            read_res?;
        }

        let frame = if color_mode == ColorMode::True {
            RgbImage::from_raw(width, height, buffer.clone()).expect("couldn't read image")
        } else {
            let mut transformed_buffer: Vec<u8> =
                Vec::with_capacity(width as usize * height as usize * 3);
            for b in &buffer {
                let [r, g, b] = PALETTE[*b as usize];
                transformed_buffer.push(r);
                transformed_buffer.push(g);
                transformed_buffer.push(b);
            }
            RgbImage::from_raw(width, height, transformed_buffer).expect("couldn't read image")
        };

        let packet = VideoPacket {
            frame,
            time: Duration::from_nanos((interval_nanos * i) as u64),
        };

        out_stream
            .feed(encoder.encode_packet(&packet).unwrap())
            .await?;

        i += 1;
    }

    out_stream.close().await
}
