use srtlib::Subtitles;
use std::fs::File;
use std::io::{self, BufReader};
use std::sync::RwLock;

use clap::Arg;

fn main() -> io::Result<()> {
    let matches = clap::App::new("ansi.moe player")
        .version("1.0")
        .author("allie signet <allie@cat-girl.gay>")
        .about("plays encoded video")
        .arg(
            Arg::with_name("INPUT")
                .help("file to play")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("framerate")
                .help("framerate at which to play video at. defaults to 23.98")
                .short("r")
                .long("framerate")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("subtitles")
                .help("subtitles file")
                .short("s")
                .long("subtitles")
                .takes_value(true),
        )
        .get_matches();

    let input_path = matches.value_of("INPUT").unwrap();
    let framerate = matches
        .value_of("framerate")
        .unwrap_or("23.98")
        .parse::<f64>()
        .expect("invalid framerate.");

    let mut subtitles = if let Some(subs_path) = matches.value_of("subtitles") {
        let mut s = Subtitles::parse_from_file(subs_path, None)
            .unwrap()
            .to_vec();
        s.reverse();
        s
    } else {
        vec![]
    };

    let input_f = File::open(input_path)?;
    let mut reader = BufReader::new(input_f);

    let stdout = io::stdout();
    let stdout_handle = stdout.lock();
    let stdout_lock = RwLock::new(stdout_handle);

    anime_telnet::player::play(&mut reader, framerate, &mut subtitles, &stdout_lock)?;

    Ok(())
}
