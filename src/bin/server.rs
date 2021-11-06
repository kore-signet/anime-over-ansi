use anime_telnet::metadata::{ColorMode, CompressionMode, VideoMetadata};
use async_compression::tokio::bufread::ZstdDecoder;
use std::io::SeekFrom;

use subparse::SubtitleEntry;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use clap::Arg;

macro_rules! break_error {
    ($ex:expr, $msg:expr) => {
        match $ex {
            Ok(_) => (),
            Err(_) => {
                eprintln!("{}", $msg);
                break;
            }
        }
    };
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
            Arg::with_name("BIND")
                .help("address to listen on")
                .required(true)
                .index(2),
        )
        .get_matches();

    let input_path = matches.value_of("INPUT").unwrap();
    let addr = matches.value_of("BIND").unwrap().to_owned();

    let input_f = File::open(input_path).await?;
    let mut reader = BufReader::new(input_f);
    let mut length_bytes: [u8; 8] = [0; 8];
    reader.read_exact(&mut length_bytes).await?;
    let metadata_length = u64::from_be_bytes(length_bytes);

    let mut metadata_bytes: Vec<u8> = vec![0; metadata_length as usize];
    reader.read_exact(&mut metadata_bytes).await?;

    let file_start_offset = reader.stream_position().await?;

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
            reader
                .seek(SeekFrom::Start(file_start_offset + track.offset))
                .await?;
            let mut subtitle_bytes: Vec<u8> = vec![0; track.length as usize];
            reader.read_exact(&mut subtitle_bytes).await?;

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
    reader
        .seek(SeekFrom::Start(file_start_offset + video_track.offset))
        .await?;

    let (tx, _rx) = broadcast::channel::<Vec<u8>>(video_track.framerate as usize * 60);
    // println!("{:?}",subtitles);]
    let tcp_tx = tx.clone();

    tokio::spawn(async move {
        let listener = TcpListener::bind(addr).await.unwrap();
        loop {
            let (mut stream, addr) = listener.accept().await.unwrap();
            eprintln!("got new client {}", addr);
            let mut msgs = tcp_tx.subscribe();
            tokio::spawn(async move {
                while let Ok(val) = msgs.recv().await {
                    break_error!(
                        stream.write_all(&val).await,
                        format!("couldn't write to client {}, closing connection.", addr)
                    );
                }
            });
        }
    });

    if video_track.compression == CompressionMode::Zstd {
        let reader = ZstdDecoder::new(reader);
        tokio::pin!(reader);
        anime_telnet::player::play(
            reader,
            video_track.framerate,
            video_track.frame_lengths,
            &mut subtitles,
            tx,
        )
        .await?;
    } else {
        tokio::pin!(reader);
        anime_telnet::player::play(
            reader,
            video_track.framerate,
            video_track.frame_lengths,
            &mut subtitles,
            tx,
        )
        .await?;
    }

    Ok(())
}
