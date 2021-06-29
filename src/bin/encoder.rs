use anime_telnet::*;
use clap::Arg;
use image::imageops;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;

use opencv::videoio::{VideoCaptureProperties,VideoCaptureTrait,VideoCapture};
use opencv::core::{Mat,Vector};

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
            Arg::with_name("resizing")
                .help("resizing algorithm to use")
                .short("r")
                .long("resize")
                .takes_value(true)
                .possible_values(&["nearest", "triangle", "gaussian", "lanczos"]),
        )
        .arg(
            Arg::with_name("no_show_frames")
                .help("don't show frames as they're encoded")
                .long("no-show-frames"),
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
    let resize_filter = match matches.value_of("resize").unwrap_or("triangle") {
        "nearest" => imageops::FilterType::Nearest,
        "triangle" => imageops::FilterType::Triangle,
        "gaussian" => imageops::FilterType::Gaussian,
        "lanczos" => imageops::FilterType::Lanczos3,
        _ => imageops::FilterType::Triangle,
    };
    let show_frames = !matches.is_present("no_show_frames");

    let input_file = matches.value_of("INPUT").unwrap();
    let out_file = matches.value_of("OUT").unwrap();

    let mut video_cap = VideoCapture::from_file(input_file, 0).expect("couldn't open video file");
    let frame_quant = video_cap.get(VideoCaptureProperties::CAP_PROP_FRAME_COUNT as i32).expect("couldn't get frame count of video") as u64;

    let cursor_pos = if show_frames { height } else { 2 };

    let out_fs = File::create(out_file).unwrap();
    let mut out = BufWriter::new(out_fs);

    let mut mat = Mat::default();
    let mut buffer: Vector<u8> = Vector::new();

    print!("\x1B[2J\x1B[1;1H");

    for i in 0..frame_quant {
        let percent_done = ((i + 1) as f64 / frame_quant as f64) * 100.0;


        print!("\x1B[{};1H", cursor_pos);
        print!("\x1B[2K");
        print!(
            "{:.2}% | Processing frame #{}/{} | reading, resizing",
            percent_done,
            i + 1,
            frame_quant
        );


        let read_frame = video_cap.read(&mut mat).unwrap();
        if !read_frame {
            break;
        }

        opencv::imgcodecs::imencode(".png", &mat, &mut buffer, &Vector::new()).unwrap();
        let mut img = image::load_from_memory_with_format(&buffer.to_vec(), image::ImageFormat::Png).expect("couldn't load image").into_rgb8();
        img = imageops::resize(&img, width, height, resize_filter);

        print!("\x1B[{};1H", cursor_pos);
        print!("\x1B[2K");
        print!(
            "{:.2}% | Processing frame #{}/{} | dithering",
            percent_done,
            i + 1,
            frame_quant
        );

        imageops::dither(&mut img, &LABAnsiColorMap);

        let frame = encode(img);

        if show_frames {
            print!("\x1B[1;1H");
            print!("{}", &frame);
            print!("\x1B[0m");
        }

        out.write_all(&frame.into_bytes())?;
        out.write_all(b".")?;
    }

    Ok(())
}
