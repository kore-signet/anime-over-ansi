use anime_telnet::*;
use clap::Arg;
use image::imageops;
use opencv::core::{Mat, Vector};
use opencv::videoio::{VideoCapture, VideoCaptureProperties, VideoCaptureTrait};
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::thread;
use std::net::*;

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
            Arg::with_name("color_mode")
                .help("ANSI color mode to use (defaults to 256color)")
                .short("c")
                .long("color")
                .takes_value(true)
                .possible_values(&["256color", "truecolor"]),
        )
        .arg(
            Arg::with_name("bind_address")
                .help("listen on a port, sending encoded video")
                .takes_value(true)
                .long("bind")
        )
        .arg(
            Arg::with_name("no_show_frames")
                .help("don't show frames as they're encoded")
                .long("no-show-frames"),
        )
        .arg(
            Arg::with_name("live_mode")
                .help("don't show progress bars, run until stopped instead of until frame end and don't save to file. for webcams and live playback")
                .short("l")
                .long("live"),
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
    let color_mode = matches.value_of("color_mode").unwrap_or("256color");
    let resize_filter = match matches.value_of("resize").unwrap_or("triangle") {
        "nearest" => imageops::FilterType::Nearest,
        "triangle" => imageops::FilterType::Triangle,
        "gaussian" => imageops::FilterType::Gaussian,
        "lanczos" => imageops::FilterType::Lanczos3,
        _ => imageops::FilterType::Triangle,
    };

    let live_mode = matches.is_present("live_mode");
    let show_frames = !matches.is_present("no_show_frames") || live_mode;

    let input_file = matches.value_of("INPUT").unwrap();
    let out_file = match matches.value_of("OUT") {
        Some(e) => e,
        None => {
            if !live_mode {
                panic!("output file required!")
            } else {
                ""
            }
        }
    };

    let mut video_cap = VideoCapture::from_file(input_file, 0).expect("couldn't open video file");

    let frame_quant = match video_cap.get(VideoCaptureProperties::CAP_PROP_FRAME_COUNT as i32) {
        Ok(f) => f as u64,
        Err(e) => {
            if live_mode {
                0_u64
            } else {
                panic!("couldn't get video frame count: {}", e)
            }
        }
    };

    let cursor_pos = if show_frames { height } else { 2 };

    let out_fs = File::create(out_file).unwrap();
    let mut out = BufWriter::new(out_fs);

    let mut mat = Mat::default();
    let mut buffer: Vector<u8> = Vector::new();
    let mut i: u64 = 0;

    let (send, recv) = multiqueue2::broadcast_queue::<String>(20);

    let server_t = if let Some(addr) = matches.value_of("bind_address").map(|s|s.to_string()) {
        let cur_recv = recv.add_stream();
        thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            for stream in listener.incoming() {
                let consumer = cur_recv.clone();
                let mut stream = stream.unwrap();
                thread::spawn(move || {
                    for msg in consumer {
                        stream.write_all(&msg.as_bytes()).unwrap();
                    }
                });
            }
        })
    } else {
        thread::spawn(||{})
    };

    recv.unsubscribe();

    print!("\x1B[2J\x1B[1;1H");

    loop {
        if !live_mode && i == frame_quant - 1 {
            break;
        }

        let percent_done = ((i + 1) as f64 / frame_quant as f64) * 100.0;

        if !live_mode {
            print!("\x1B[{};1H", cursor_pos);
            print!("\x1B[2K");
            print!(
                "{:.2}% | Processing frame #{}/{} | reading, resizing",
                percent_done,
                i + 1,
                frame_quant
            );
        }

        let read_frame = video_cap.read(&mut mat).unwrap();
        if !read_frame {
            break;
        }

        opencv::imgcodecs::imencode(".png", &mat, &mut buffer, &Vector::new()).unwrap();
        let mut img =
            image::load_from_memory_with_format(&buffer.to_vec(), image::ImageFormat::Png)
                .expect("couldn't load image")
                .into_rgb8();

        img = imageops::resize(&img, width, height, resize_filter);

        if !live_mode {
            print!("\x1B[{};1H", cursor_pos);
            print!("\x1B[2K");
            print!(
                "{:.2}% | Processing frame #{}/{} | dithering",
                percent_done,
                i + 1,
                frame_quant
            );
        }

        if color_mode == "256color" {
            imageops::dither(&mut img, &LABAnsiColorMap);
        }

        let frame = encode(img, color_mode);

        if show_frames {
            let s = format!("\x1B[1;1H{}\x1b[0m", &frame);
            print!("{}",s);
            send.try_send(s).unwrap();
        }

        if !live_mode {
            out.write_all(&frame.into_bytes())?;
            out.write_all(b".")?;
        }

        if !live_mode {
            i += 1;
        }
    }

    drop(send);
    server_t.join();

    Ok(())
}
