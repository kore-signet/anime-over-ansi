use anime_telnet::*;
use clap::Arg;
use image::imageops;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::process::Command;
use std::path::PathBuf;

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

    let frames_out = Command::new("ffprobe")
                                .args(&["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0", input_file])
                                .output()
                                .expect("couldn't count frames with ffprobe");

    let frame_quant_s = String::from_utf8(frames_out.stdout).unwrap();
    let frame_quant = frame_quant_s.trim_end().parse::<u64>().expect("couldn't parse frame number as unsigned integer");

    let cursor_pos = if show_frames { height } else { 2 };

    let out_fs = File::create(out_file).unwrap();
    let mut out = BufWriter::new(out_fs);

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

        let frame_out = Command::new("ffmpeg")
                                .args(&["-i", input_file, "-vf", &format!("select=eq(n\\,{})",i), "-vframes", "1", "-c:v", "png", "-f", "image2pipe", "-"])
                                .output()
                                .expect("couldn't extract frame with ffmpeg");

        let mut img = image::load_from_memory_with_format(&frame_out.stdout, image::ImageFormat::Png).expect("couldn't load image").into_rgb8();
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
