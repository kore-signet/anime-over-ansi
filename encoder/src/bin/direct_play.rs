use clap::clap_app;

use futures::{FutureExt, StreamExt};
use player::subtitles::SSAParser;
use player::{play, PacketFilterTransformer};
use tokio_stream::wrappers::ReceiverStream;

use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use container::metadata::{ColorMode, SubtitleFormat};
use container::packet::*;
use encoder::tool_utils::*;
use encoder::video_encoder::*;
use encoder::*;
use postage::prelude::*;

fn main() -> anyhow::Result<()> {
    let matches = clap_app!(encoder =>
        (version: "1.0")
        (author: "emily signet <emily@cat-girl.gay>")
        (@arg INPUT: +required "input file or url for the encoder")
        (@arg SHOW_SSA_NAMES: --subtitle_names "show subtitle entry character names in ssa subtitles (not always used)")
        (@arg SHOW_SSA_LAYERS: --ssa_layer ... +takes_value "ssa layers to show (all if not passed)")
    )
    .get_matches();

    let ssa_layers = matches
        .values_of("SHOW_SSA_LAYERS")
        .map(|s| {
            s.filter_map(|v| v.parse::<isize>().ok())
                .collect::<Vec<isize>>()
        })
        .unwrap_or_default();
    let show_ssa_names = matches.is_present("SHOW_SSA_NAMES");

    let theme = dialoguer::theme::ColorfulTheme::default();
    let ff_source = FFMpegSource::open_url(matches.value_of("INPUT").unwrap())?;

    let mut video_sources = Vec::new();
    let mut subtitle_sources = Vec::new();

    for (i, stream) in ff_source.streams().iter().enumerate() {
        if let Some(kind) = SourceKind::from_parameters(stream.codec_parameters()) {
            let meta = SourceStreamMetadata {
                idx: i,
                source_kind: kind,
                codec_name: stream.codec_parameters().decoder_name(),
                title: stream.get_metadata("title"),
            };

            if meta.source_kind == SourceKind::Video {
                video_sources.push(meta);
            } else if meta.source_kind == SourceKind::Subtitles {
                subtitle_sources.push(meta);
            }
        }
    }

    let mut tracks: Vec<AnsiTrack> = Vec::with_capacity(2);
    let mut video_tracks = Vec::new();
    let mut subtitle_tracks = Vec::new();

    tracks.push(cli::select_video_track(&video_sources, 0)?);

    if dialoguer::Select::with_theme(&theme)
        .item("add subtitles")
        .item("finish & play")
        .interact()?
        == 0
    {
        tracks.push(cli::select_subtitle_track(&subtitle_sources, 1)?);
    };

    let rt = tokio::runtime::Runtime::new().unwrap();

    let (encoded_packet_tx, mut encoded_packet_rx) =
        tokio::sync::mpsc::channel::<container::packet::Packet<BytesMut>>(255);
    let (source_packet_pipe, source_packet_receiver) =
        postage::broadcast::channel::<Arc<FFMpegPacket>>(255);

    let mut pipes = Vec::with_capacity(tracks.len());

    for track in tracks.iter().cloned() {
        match track {
            AnsiTrack::VideoTrack(t) => {
                let decoder = FFMpegVideoDecoder::from_stream(
                    &ff_source.streams()[t.source_stream_index],
                    ac_ffmpeg::codec::video::scaler::Algorithm::Lanczos,
                    t.track_width,
                    t.track_height,
                )
                .unwrap();

                let encoder = FrameEncoder {
                    stream_index: t.track_id as u16,
                    width: t.track_width as u32,
                    height: t.track_height as u32,
                    color: t.color_mode,
                    use_diffing: false,
                    last_frame: None,
                };

                match t.color_mode {
                    ColorMode::True => {
                        pipes.push(pipeline! {
                            receive from source_packet_receiver;
                            send to encoded_packet_tx;
                            stream t.source_stream_index => decoder => passthrough => encoder
                        });
                    }
                    ColorMode::EightBit => {
                        pipes.push(pipeline! {
                            receive from source_packet_receiver;
                            send to encoded_packet_tx;
                            stream t.source_stream_index => decoder => t.dither_mode.build() => encoder
                        });
                    }
                }

                video_tracks.push(container::metadata::VideoTrack {
                    name: Some(t.track_name.clone()),
                    color_mode: t.color_mode,
                    height: t.track_height as u32,
                    width: t.track_width as u32,
                    codec_private: None,
                    index: t.track_id as u16,
                })
            }
            AnsiTrack::SubtitleTrack(t) => {
                pipes.push(pipeline! {
                    receive from source_packet_receiver;
                    send to encoded_packet_tx;
                    stream t.source_stream_index => GenericPacketDecoder::override_stream_index(t.track_id as u16) => passthrough => passthrough
                });

                let parameters = ff_source.streams()[t.source_stream_index].codec_parameters();

                let mut codec_private = parameters.extradata().map(|v| v.to_vec());

                // if we're demuxing a matroska file, correct the codec private part to have the properly ordered ssa header
                if ff_source
                    .get_format_names()
                    .filter(|v| v.contains("matroska"))
                    .is_some()
                {
                    if let Some(codec_private) = codec_private.as_mut() {
                        let mut contents = String::from_utf8_lossy(&codec_private)
                            .trim_end()
                            .to_string();

                        *codec_private = {
                            let mut codec_private = String::new();
                            while let Ok((input, (section_str, section))) =
                                substation::parser::section_with_input(&contents)
                            {
                                if let Some(h) = section.as_event_header() {
                                    codec_private += "[Events]\n";
                                    codec_private += "Format: ReadOrder, Layer, Style, Name, MarginL, MarginR, MarginV, Effect, Text";
                                    codec_private += "\n\n";
                                } else {
                                    // ReadOrder, Layer, Style, Name, MarginL, MarginR, MarginV, Effect, Text 
                                    codec_private += section_str.trim_end();
                                    codec_private += "\n\n";
                                }

                                contents = input.trim_start().to_owned();
                            }

                            codec_private
                        }.into_bytes();
                    }
                }

                subtitle_tracks.push(container::metadata::SubtitleTrack {
                    name: Some(t.track_name.clone()),
                    lang: None,
                    format: SubtitleFormat::from_codec_name(
                        parameters.encoder_name().map(|v| v).unwrap_or("unknown"),
                    ),
                    codec_private: codec_private,
                    index: t.track_id as u16,
                })
            }
        }
    }

    drop(source_packet_receiver);
    drop(encoded_packet_tx);

    let mut subtitle_mapper: Box<dyn PacketFilterTransformer + Send> = if let Some(codec_private) =
        subtitle_tracks
            .iter()
            .next()
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

    let router = route_source(source_packet_pipe, ff_source, pipes);

    rt.block_on(async move {
        let (video_tx, video_rx) = tokio::sync::mpsc::channel::<Packet<Bytes>>(255);
        let (subtitle_tx, subtitle_rx) = tokio::sync::mpsc::channel::<Packet<Bytes>>(8000);

        let player_task = tokio::task::spawn(play(
            ReceiverStream::new(video_rx),
            ReceiverStream::new(subtitle_rx)
                .filter_map(move |f| futures::future::ready(subtitle_mapper.filter_map_packet(f))),
        ));

        let sender_task = tokio::task::spawn(async move {
            while let Some(packet) = encoded_packet_rx.recv().await {
                let packet = packet.freeze();

                match packet.stream_index {
                    y if y == 0 => video_tx.send(packet).await,
                    x if x == 1 => subtitle_tx.send(packet).await,
                    _ => continue,
                };
            }
        });

        tokio::join!(router, player_task, sender_task);
    });

    Ok(())
}
