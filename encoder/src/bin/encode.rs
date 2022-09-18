use clap::clap_app;

use futures::FutureExt;

use std::sync::Arc;

use bytes::BytesMut;
use container::metadata::{ColorMode, CompressionMode, SubtitleFormat};
use container::packet::*;
use encoder::tool_utils::*;
use encoder::video_encoder::*;
use encoder::*;
use postage::prelude::*;
use tokio::{fs::File, io::BufWriter};

fn main() -> anyhow::Result<()> {
    let matches = clap_app!(encoder =>
        (version: "1.0")
        (author: "emily signet <emily@cat-girl.gay>")
        (@arg INPUT: +required "input file or url for the encoder")
        (@arg OUTPUT: -o --output +takes_value +required "output file")
        (@arg COMPRESSION_LEVEL: --compression +takes_value)
        (@arg NOCOMPRESSION: --no-compress)
    )
    .get_matches();

    let compression_level = matches
        .value_of("COMPRESSION_LEVEL")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(5);
    let compress = !matches.is_present("NOCOMPRESSION");

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

    loop {
        match dialoguer::Select::with_theme(&theme)
            .item("add video track")
            .item("add subtitle track")
            .item("finalize & render")
            .interact()?
        {
            0 => {
                tracks.push(cli::select_video_track(&video_sources, tracks.len() + 1)?);
            }
            1 => {
                tracks.push(cli::select_subtitle_track(
                    &subtitle_sources,
                    tracks.len() + 1,
                )?);
            }
            2 => break,
            _ => unreachable!(),
        }
    }

    let mut video_tracks = Vec::new();
    let mut subtitle_tracks = Vec::new();

    let rt = tokio::runtime::Runtime::new().unwrap();

    let (encoded_packet_tx, encoded_packet_rx) =
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

    let video_metadata = container::metadata::VideoMetadata {
        video_tracks,
        subtitle_tracks,
        attachments: Vec::new(),
        compression: if compress {
            CompressionMode::Zstd
        } else {
            CompressionMode::None
        },
    };

    let router = route_source(source_packet_pipe, ff_source, pipes);
    let (state_tx, mut state_rx) = tokio::sync::watch::channel((0.0, 1));

    rt.block_on(async move {
        let output_file = BufWriter::new(
            File::create(matches.value_of("OUTPUT").unwrap())
                .await
                .unwrap(),
        );

        let writer = if compress {
            #[cfg(feature = "compression")]
            {
                write_with_container_metadata(
                    video_metadata,
                    output_file,
                    encoded_packet_rx,
                    state_tx,
                    PacketCompressor::with_level(compression_level).unwrap(),
                )
                .boxed()
            }

            #[cfg(not(feature = "compression"))]
            {
                write_with_container_metadata(
                    video_metadata,
                    output_file,
                    encoded_packet_rx,
                    state_tx,
                    (),
                )
                .boxed()
            }
        } else {
            write_with_container_metadata(
                video_metadata,
                output_file,
                encoded_packet_rx,
                state_tx,
                (),
            )
            .boxed()
        };

        tokio::task::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut stdout = tokio::io::stdout();

            while state_rx.changed().await.is_ok() {
                let (fps, total) = *state_rx.borrow();
                stdout
                    .write_all(format!("\x1b[2K\rframe {total} - fps {fps:.1}").as_bytes())
                    .await;
                stdout.flush().await;
                tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
            }
        });

        tokio::join!(router, writer);
    });

    Ok(())
}
