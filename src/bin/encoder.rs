use anime_telnet::*;
use clap::Arg;
use image::imageops;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let matches = clap::App::new("anime over telnet encoder")
        .version("1.0")
        .author("allie signet <allie@cat-girl.gay>")
        .about("encodes video into ANSI escape sequences")
        .arg(
            Arg::with_name("VIDEO_FOLDER")
                .help("folder with the video's frames, as images, in it")
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
            Arg::with_name("subtitles")
                .help("subtitles to burn in")
                .short("s")
                .long("subtitles")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("framerate")
                .help("if burning in subtitles, specify framerate to calculate intervals")
                .short("r")
                .long("fps")
                .takes_value(true),
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

    let input_folder = matches.value_of("VIDEO_FOLDER").unwrap();
    let out_file = matches.value_of("OUT").unwrap();

    let mut entries: Vec<PathBuf> = fs::read_dir(input_folder)
        .expect("couldn't read input folder")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().unwrap_or(OsStr::new("no")) == "png")
        .collect();

    entries.sort();

    let frame_quant = entries.len();
    let cursor_pos = if show_frames { height } else { 2 };

    let out_fs = File::create(out_file).unwrap();
    let mut out = BufWriter::new(out_fs);

    print!("\x1B[2J\x1B[1;1H");

    for (i, file_path) in entries.into_iter().enumerate() {
        let percent_done = ((i + 1) as f64 / frame_quant as f64) * 100.0;

        print!("\x1B[{};1H", cursor_pos);
        print!("\x1B[2K");
        print!(
            "{:.2}% | Processing frame #{}/{} | reading, resizing",
            percent_done,
            i + 1,
            frame_quant
        );
        let mut img = image::open(file_path).unwrap().into_rgb8();
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
