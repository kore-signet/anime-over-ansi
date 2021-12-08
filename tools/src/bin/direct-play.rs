use anime_telnet::{encoding::*, metadata::*, palette::AnsiColorMap, subtitles::SSAFilter};
use anime_telnet_encoder::ANSIVideoEncoder;
use clap::Arg;
use play::{player, subtitles::SubtitlePacket};

use cyanotype::*;
use std::collections::HashSet;

use fast_image_resize as fr;

use futures::stream::{self, Stream, StreamExt};

use std::pin::Pin;
use tokio::io::{self, AsyncWriteExt, BufWriter};
use tokio::net::TcpListener;
use tokio::task::{self, JoinHandle};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("ansi.moe direct player")
        .version("1.0")
        .author("allie signet <allie@cat-girl.gay>")
        .about("encodes and directly plays normal video")
        .arg(
            Arg::with_name("INPUT")
                .help("file to read from")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("resizing")
                .help("resizing algorithm to use")
                .long("resize")
                .takes_value(true)
                .possible_values(&[
                    "nearest",
                    "hamming",
                    "catmullrom",
                    "mitchell",
                    "lanczos",
                    "bilinear",
                ]),
        )
        .arg(
            Arg::with_name("bind")
                .long("bind")
                .takes_value(true)
                .help("bind a TCP server to specified address instead of outputting to stdout"),
        )
        .arg(
            Arg::with_name("filter_ssa_layers")
            .long("ssa-layers")
            .takes_value(true)
            .multiple(true)
            .help("only shows subtitles on the specified layers, if using a SubStation Alpha stream.")
        )
        .arg(
            Arg::with_name("filter_ssa_styles")
            .long("ssa-styles")
            .takes_value(true)
            .multiple(true)
            .help("only shows subtitles with the specified styles, if using a SubStation Alpha stream.")
        )
        .arg(
            Arg::with_name("diff")
            .long("diff")
            .help("use diffing algorithm when optimal")
        )
        .arg(
            Arg::with_name("pattern_percent")
            .long("dither-percent")
            .takes_value(true)
            .help("error calculation % for pattern dithering")
        )
        .get_matches();

    ac_ffmpeg::set_log_callback(|_, _| {
        // println!("ffmpeg: {}", m);
    });

    let pattern_percent = if let Some(v) = matches.value_of("pattern_percent") {
        (v.parse::<f64>().expect("invalid percentage") * 100.0) as u32
    } else {
        900
    };

    let mut demuxer = Demuxer::from_url(matches.value_of("INPUT").unwrap()).unwrap();
    demuxer.block_video_streams(true);
    let theme = dialoguer::theme::ColorfulTheme::default();

    let width = dialoguer::Input::with_theme(&theme)
        .with_prompt("video width")
        .default(192u32)
        .interact_text()
        .unwrap();
    let height = dialoguer::Input::with_theme(&theme)
        .with_prompt("video height")
        .default(108u32)
        .interact_text()
        .unwrap();
    let color_mode = match dialoguer::Select::with_theme(&theme)
        .with_prompt("color mode")
        .items(&["8bit", "full color"])
        .interact()
        .unwrap()
    {
        0 => ColorMode::EightBit,
        1 => ColorMode::True,
        _ => panic!(),
    };
    let color_mapping = match dialoguer::Select::with_theme(&theme)
        .with_prompt("color mapping equation")
        .items(&[
            "DeltaE76 (fastest)",
            "DeltaE94 (slower, may be more accurate)",
        ])
        .default(0)
        .interact()
        .unwrap()
    {
        0 => AnsiColorMap::CIE76,
        1 => AnsiColorMap::CIE94,
        _ => panic!(),
    };

    let dither_mode = if color_mode == ColorMode::EightBit {
        [
            DitherMode::FloydSteinberg(color_mapping),
            DitherMode::Pattern(color_mapping, 2, pattern_percent),
            DitherMode::Pattern(color_mapping, 4, pattern_percent),
            DitherMode::Pattern(color_mapping, 8, pattern_percent),
        ][dialoguer::Select::with_theme(&theme)
            .with_prompt("dithering mode")
            .items(&[
                "floyd-steinberg",
                "ordered pattern dithering (2x2)",
                "ordered pattern dithering (4x4)",
                "ordered pattern dithering (8x8)",
            ])
            .interact()
            .unwrap()]
    } else {
        DitherMode::None
    };

    let mut encoder = ANSIVideoEncoder {
        stream_index: 0,
        width,
        height,
        color_mode,
        dither_mode,
        diff: matches.is_present("diff"),
        encoder_opts: PacketFlags {
            compression_level: None,
            compression_mode: CompressionMode::None,
            is_keyframe: true,
        },
        last_frame: None,
    };

    let mut subtitle_stream_indexes = demuxer
        .subtitle_streams
        .keys()
        .copied()
        .collect::<Vec<usize>>();

    subtitle_stream_indexes.sort_unstable();

    let subtitle_stream_idx = dialoguer::Select::with_theme(&theme)
        .with_prompt("subtitle track")
        .items(
            &subtitle_stream_indexes
                .iter()
                .map(|v| {
                    let stream = &demuxer.subtitle_streams[v];
                    let metadata = stream.metadata();
                    format!(
                        "Stream {}, {} ({})",
                        v,
                        metadata.get("title").unwrap_or(&"<undefined>"),
                        stream.parameters().decoder_name().unwrap_or("<undefined>")
                    )
                })
                .chain(vec!["none".to_owned()].into_iter())
                .collect::<Vec<String>>(),
        )
        .interact()
        .unwrap();

    let subtitle_stream: Pin<Box<dyn Stream<Item = SubtitlePacket> + Send>> =
        if subtitle_stream_idx < subtitle_stream_indexes.len() {
            if demuxer.subtitle_streams[&subtitle_stream_indexes[subtitle_stream_idx]]
                .parameters()
                .decoder_name()
                .map(|v| v == "ssa" || v == "ass")
                .unwrap_or(false)
            {
                let ssa_filter = SSAFilter {
                    layers: matches
                        .values_of("filter_ssa_layers")
                        .map(|v| {
                            v.map(|i| i.parse::<isize>().expect("invalid ssa layer number"))
                                .collect::<Vec<isize>>()
                        })
                        .unwrap_or_default(),
                    styles: matches
                        .values_of("filter_ssa_styles")
                        .map(|v| v.map(|s| s.to_owned()).collect::<Vec<String>>())
                        .unwrap_or_default(),
                };

                Box::pin(
                    demuxer
                        .subscribe_to_subtitles(subtitle_stream_indexes[subtitle_stream_idx])
                        .unwrap()
                        .filter_map(move |v| match v {
                            cyanotype::SubtitlePacket::SSAEntry(entry) => {
                                if ssa_filter.check(&entry) {
                                    futures::future::ready(Some(SubtitlePacket::SSAEntry(entry)))
                                } else {
                                    futures::future::ready(None)
                                }
                            }
                            cyanotype::SubtitlePacket::SRTEntry(entry) => {
                                futures::future::ready(Some(SubtitlePacket::SRTEntry(entry)))
                            }
                            _ => futures::future::ready(None),
                        })
                        .boxed(),
                )
            } else {
                demuxer
                    .subscribe_to_subtitles(subtitle_stream_indexes[subtitle_stream_idx])
                    .unwrap()
                    .filter_map(|v| match v {
                        cyanotype::SubtitlePacket::SSAEntry(entry) => {
                            futures::future::ready(Some(SubtitlePacket::SSAEntry(entry)))
                        }
                        cyanotype::SubtitlePacket::SRTEntry(entry) => {
                            futures::future::ready(Some(SubtitlePacket::SRTEntry(entry)))
                        }
                        _ => futures::future::ready(None),
                    })
                    .boxed()
            }
        } else {
            stream::iter(vec![]).boxed()
        };

    let resize_filter = match matches.value_of("resize").unwrap_or("hamming") {
        "nearest" => fr::FilterType::Box,
        "bilinear" => fr::FilterType::Bilinear,
        "hamming" => fr::FilterType::Hamming,
        "catmullrom" => fr::FilterType::CatmullRom,
        "mitchell" => fr::FilterType::Mitchell,
        "lanczos" => fr::FilterType::Lanczos3,
        _ => fr::FilterType::Hamming,
    };

    let processor = ProcessorPipeline {
        width: encoder.width,
        height: encoder.height,
        filter: resize_filter,
        dither_modes: HashSet::from([encoder.dither_mode]),
    };

    let video_stream = demuxer
        .subscribe_to_video(*demuxer.video_streams.keys().next().unwrap())
        .unwrap();

    demuxer.block_video_streams(false);

    let demuxer_task = tokio::task::spawn(async move {
        demuxer.run().await.unwrap();
    });

    let resized_stream = video_stream.map(move |img| {
        let time = img.time;
        let frame = processor.process(&img.frame).remove(0).1;

        encoder.encode_packet(&VideoPacket { frame, time }).unwrap()
    });

    let (mut otx, mut orx) = async_broadcast::broadcast::<Vec<u8>>(64);
    otx.set_overflow(true);

    let output_task: JoinHandle<io::Result<()>> =
        if let Some(addr) = matches.value_of("bind").map(|v| v.to_owned()) {
            task::spawn(async move {
                let listener = TcpListener::bind(addr).await?;
                let mut sockets = Vec::new();
                let mut to_rm = Vec::new();

                loop {
                    tokio::select! {
                        Ok((mut socket,addr)) = listener.accept() => {
                            if socket.write_all(b"\x1B[2J\x1B[1;1H").await.is_ok() {
                                sockets.push(BufWriter::new(socket));
                                println!("got new connection from {}", addr);
                                println!("total connections: {}", sockets.len());
                            };
                        },
                        Ok(msg) = orx.recv() => {
                            if !to_rm.is_empty() {
                                println!("disconnecting {} broken socket(s)", to_rm.len());
                            }

                            for i in to_rm.drain(..) {
                                sockets.remove(i).into_inner().shutdown().await;
                            }

                            for (i,socket) in sockets.iter_mut().enumerate() {
                                if socket.write_all(&msg).await.is_err() {
                                    to_rm.push(i);
                                };
                            }
                        },
                        else => break
                    }
                }

                Ok(())
            })
        } else {
            task::spawn(async move {
                print!("\x1B[2J\x1B[1;1H");

                let mut stdout = io::stdout();
                while let Some(val) = orx.next().await {
                    stdout.write_all(&val).await?;
                }

                Ok(())
            })
        };

    let runner = task::spawn(player::play(resized_stream.boxed(), subtitle_stream, otx));

    tokio::try_join! {
        demuxer_task,
        runner,
        output_task
    };

    Ok(())
}
