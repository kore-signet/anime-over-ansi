use anime_telnet::{
    encoding::PacketDecoder,
    metadata::{SubtitleFormat, VideoMetadata},
};
use anime_telnet_encoder::{
    player,
    subtitles::{SRTDecoder, SSADecoder},
    PacketReadCodec,
};
use clap::Arg;
use cyanotype::SubtitlePacket;
use dialoguer::{theme::ColorfulTheme, Select};
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use rmp_serde as rmps;
use tokio::io::AsyncReadExt;
use tokio::task;
use tokio_util::codec::FramedRead;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("ansi.moe inspector")
        .version("1.0")
        .author("allie signet <allie@cat-girl.gay>")
        .about("inspects .ansi video container")
        .arg(
            Arg::with_name("INPUT")
                .help("file to read from")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("show_packets")
                .long("--show-packets")
                .takes_value(true)
                .multiple(true)
                .help("Show data for individual packets from the specified streams"),
        )
        .get_matches();

    let mut input_fs = tokio::fs::File::open(matches.value_of("INPUT").unwrap()).await?;
    let metadata_len = input_fs.read_u64().await?;
    let mut metadata_bytes = vec![0; metadata_len as usize];
    input_fs.read_exact(&mut metadata_bytes).await?;
    let mut metadata: VideoMetadata = rmps::from_read_ref(&metadata_bytes).unwrap();

    let video_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("choose video track")
        .items(
            &metadata
                .video_tracks
                .iter()
                .map(|v| {
                    format!(
                        "{} ({}x{}, color {}, compression: {})",
                        v.name.as_ref().unwrap_or(&"<undefined>".to_owned()),
                        v.width,
                        v.height,
                        v.color_mode,
                        v.compression
                    )
                })
                .collect::<Vec<String>>(),
        )
        .interact()
        .unwrap();

    let video_track = metadata.video_tracks.remove(video_selection);
    let video_track_index = video_track.index;

    let subtitle_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("choose subtitle track")
        .items(
            &metadata
                .subtitle_tracks
                .iter()
                .map(|v| {
                    format!(
                        "{} ({})",
                        v.name.as_deref().unwrap_or("<undefined>"),
                        v.format
                    )
                })
                .chain(vec!["none".to_owned()].into_iter())
                .collect::<Vec<String>>(),
        )
        .interact()
        .unwrap();

    let subtitle_track = if subtitle_selection < metadata.subtitle_tracks.len() {
        Some(metadata.subtitle_tracks.remove(subtitle_selection))
    } else {
        None
    };

    let has_subtitle_track = subtitle_track.is_some();
    let subtitle_track_index = subtitle_track.as_ref().map(|v| v.index).unwrap_or(0);

    let mut subtitle_decoder: Option<Box<dyn PacketDecoder<Output = SubtitlePacket>>> =
        if let Some(track) = subtitle_track {
            match track.format {
                SubtitleFormat::SubStationAlpha => Some(Box::new(SSADecoder::new(
                    vec![
                        "ReadOrder",
                        "Layer",
                        "Style",
                        "Name",
                        "MarginL",
                        "MarginR",
                        "MarginV",
                        "Effect",
                        "Text",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ))),
                SubtitleFormat::SubRip => Some(Box::new(SRTDecoder::new())),
                _ => None,
            }
        } else {
            None
        };

    let mut packet_stream = FramedRead::new(input_fs, PacketReadCodec::new(true));
    let (mut stx, srx) = mpsc::channel(64);
    let (mut vtx, vrx) = mpsc::channel(64);
    if !has_subtitle_track {
        stx.close().await.unwrap();
    }

    let runner = task::spawn(player::play(Box::pin(vrx), Box::pin(srx)));

    while let Some(packet) = packet_stream.next().await {
        let packet = packet?;
        if packet.stream_index == video_track_index {
            vtx.send(packet).await.unwrap();
        } else if has_subtitle_track && packet.stream_index == subtitle_track_index {
            if let Some(packige) = subtitle_decoder.as_mut().unwrap().decode_packet(packet) {
                stx.send(packige).await.unwrap();
            }
        }
    }

    runner.await.unwrap().unwrap();

    Ok(())
}
