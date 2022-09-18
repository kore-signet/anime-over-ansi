use bytes::Bytes;
use clap::clap_app;
use container::{
    codec::PacketDecoder,
    metadata::{CompressionMode, SubtitleFormat, VideoMetadata},
    packet::{Packet, PacketMapper},
};

use futures::StreamExt;
use player::{play, subtitles::SSAParser, PacketDecompressor, PacketFilterTransformer};

use tokio::{fs::File, io::AsyncReadExt};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::codec::FramedRead;

use std::env;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = clap_app!(encoder =>
        (version: "1.0")
        (author: "emily signet <emily@cat-girl.gay>")
        (@arg INPUT: +required "input file or url for the encoder")
        (@arg SHOW_SSA_NAMES: --subtitle_names "show subtitle entry character names in ssa subtitles (not always used)")
        (@arg SHOW_SSA_LAYERS: --ssa_layer ... +takes_value "ssa layers to show (all if not passed)")
    )
    .get_matches();

    let mut input = File::open(matches.value_of("INPUT").unwrap()).await?;

    let ssa_layers = matches
        .values_of("SHOW_SSA_LAYERS")
        .map(|s| {
            s.filter_map(|v| v.parse::<isize>().ok())
                .collect::<Vec<isize>>()
        })
        .unwrap_or_default();
    let show_ssa_names = matches.is_present("SHOW_SSA_NAMES");

    let meta_len = input.read_u64_le().await?;
    let mut metadata = vec![0u8; meta_len as usize];
    input.read_exact(&mut metadata).await?;

    let metadata: VideoMetadata = rmp_serde::from_slice(&metadata).unwrap();
    let video_tracks_display = metadata
        .video_tracks
        .iter()
        .map(|v| {
            format!(
                "track #{} - {} ({}x{}) - color {}",
                v.index,
                v.name.clone().unwrap_or("unknown".to_owned()),
                v.width,
                v.height,
                v.color_mode
            )
        })
        .collect::<Vec<String>>();

    let subtitle_tracks_display = metadata
        .subtitle_tracks
        .iter()
        .map(|v| {
            format!(
                "track #{} - {}",
                v.index,
                v.name.clone().unwrap_or("unknown".to_owned()),
            )
        })
        .collect::<Vec<String>>();

    let video_track_idx = metadata
        .video_tracks
        .get(
            dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("select video track")
                .items(&video_tracks_display)
                .interact()
                .unwrap(),
        )
        .unwrap()
        .index;

    let (has_subtitles, subtitle_track_idx) = if !subtitle_tracks_display.is_empty() {
        if let Some(track) = metadata.subtitle_tracks.get(
            dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("select subtitle track")
                .items(&subtitle_tracks_display)
                .item("none")
                .interact()
                .unwrap(),
        ) {
            (true, track.index)
        } else {
            (false, 0)
        }
    } else {
        (false, 0)
    };

    let mapper: Box<dyn PacketMapper> = if metadata.compression == CompressionMode::Zstd {
        Box::new(PacketDecompressor::new()?)
    } else {
        Box::new(())
    };

    let mut subtitle_mapper: Box<dyn PacketFilterTransformer + Send> = if let Some(codec_private) =
        metadata
            .subtitle_tracks
            .iter()
            .find(|v| subtitle_track_idx == v.index)
            .filter(|v| v.format == SubtitleFormat::SubStationAlpha)
            .and_then(|v| v.codec_private.as_ref())
    {
        let subtitle_filter = SSAParser::with_filter(
            String::from_utf8_lossy(codec_private).to_string(),
            show_ssa_names,
            move |entry| {
                if let Some(entry_layer) = entry.layer {
                    ssa_layers.is_empty() || ssa_layers.contains(&entry_layer)
                } else {
                    true
                }
            },
        );
        if let Some(filter) = subtitle_filter {
            Box::new(filter)
        } else {
            Box::new(())
        }
    } else {
        Box::new(())
    };

    let mut framed_read = FramedRead::new(input, PacketDecoder::with_mapper(mapper));

    let (video_tx, video_rx) = tokio::sync::mpsc::channel::<Packet<Bytes>>(255);
    let (subtitle_tx, subtitle_rx) = tokio::sync::mpsc::channel::<Packet<Bytes>>(8000);

    tokio::task::spawn(play(
        ReceiverStream::new(video_rx),
        ReceiverStream::new(subtitle_rx)
            .filter_map(move |f| futures::future::ready(subtitle_mapper.filter_map_packet(f))),
    ));

    while let Some(packet_res) = framed_read.next().await {
        let packet = packet_res.unwrap();
        match packet.stream_index {
            y if y == video_track_idx => video_tx.send(packet).await,
            x if has_subtitles && x == subtitle_track_idx => subtitle_tx.send(packet).await,
            _ => continue,
        };
    }

    Ok(())
}
