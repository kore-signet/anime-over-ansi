use anime_telnet::metadata::{ColorMode, CompressionMode, VideoMetadata};
use std::fs::File;
use std::io::{self, prelude::*, BufReader, SeekFrom};
use std::sync::RwLock;
use subparse::SubtitleEntry;

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
        .get_matches();

    let input_path = matches.value_of("INPUT").unwrap();

    let input_f = File::open(input_path)?;
    let mut reader = BufReader::new(input_f);
    let mut length_bytes: [u8; 8] = [0; 8];
    reader.read_exact(&mut length_bytes)?;
    let metadata_length = u64::from_be_bytes(length_bytes);

    let mut metadata_bytes: Vec<u8> = vec![0; metadata_length as usize];
    reader.read_exact(&mut metadata_bytes)?;

    let file_start_offset = reader.stream_position().unwrap();

    let mut metadata: VideoMetadata = serde_json::from_slice(&metadata_bytes).unwrap();

    let video_track_n = dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt("video track")
        .items(
            &metadata
                .video_tracks
                .iter()
                .map(|track| {
                    format!(
                        "{name}, {framerate:.2} fps, {color}, {width}x{height}",
                        name = track.name.clone().unwrap_or_else(|| "unnamed".to_owned()),
                        framerate = track.framerate,
                        color = if track.color_mode == ColorMode::EightBit {
                            "eight bit"
                        } else {
                            "true color"
                        },
                        width = track.width,
                        height = track.height
                    )
                })
                .collect::<Vec<String>>(),
        )
        .interact()
        .unwrap();

    let mut subtitles: Vec<SubtitleEntry> = Vec::new();

    if !metadata.subtitle_tracks.is_empty() {
        let subtitle_track_n =
            dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("subtitle track")
                .items(
                    &metadata
                        .subtitle_tracks
                        .iter()
                        .map(|v| {
                            format!(
                                "{name}, language: {lang}",
                                name = v.name.clone().unwrap_or_else(|| "unnamed".to_owned()),
                                lang = v.lang.clone().unwrap_or_else(|| "unspecified".to_owned()),
                            )
                        })
                        .collect::<Vec<String>>(),
                )
                .item("no subtitles")
                .interact()
                .unwrap();

        if subtitle_track_n < metadata.subtitle_tracks.len() {
            let track = &metadata.subtitle_tracks[subtitle_track_n];
            reader.seek(SeekFrom::Start(file_start_offset + track.offset))?;
            let mut subtitle_bytes: Vec<u8> = vec![0; track.length as usize];
            reader.read_exact(&mut subtitle_bytes)?;

            subtitles = subparse::parse_bytes(
                track.format,
                &subtitle_bytes,
                None,
                metadata.video_tracks[video_track_n].framerate,
            )
            .unwrap()
            .get_subtitle_entries()
            .unwrap();

            subtitles.sort_by_key(|s| s.timespan.start.msecs());
            subtitles.reverse();
        }
    }

    let video_track = metadata.video_tracks.remove(video_track_n);
    reader.seek(SeekFrom::Start(file_start_offset + video_track.offset))?;

    let stdout = io::stdout();
    let stdout_handle = stdout.lock();
    let stdout_lock = RwLock::new(stdout_handle);

    // println!("{:?}",subtitles);

    if video_track.compression == CompressionMode::Zstd {
        let mut reader = zstd::Decoder::with_buffer(reader)?;
        anime_telnet::player::play(
            &mut reader,
            video_track.framerate,
            video_track.frame_lengths,
            &mut subtitles,
            &stdout_lock,
        )?;
    } else {
        anime_telnet::player::play(
            &mut reader,
            video_track.framerate,
            video_track.frame_lengths,
            &mut subtitles,
            &stdout_lock,
        )?;
    }

    Ok(())
}
