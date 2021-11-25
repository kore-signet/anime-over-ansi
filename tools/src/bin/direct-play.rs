use anime_telnet::{encoding::*, metadata::*};
use anime_telnet_encoder::{player, subtitles::SSAFilter, ANSIVideoEncoder};
use clap::Arg;

use cyanotype::*;
use std::collections::HashSet;
use std::fs::File;

use fast_image_resize as fr;

use futures::stream::{self, Stream, StreamExt};

use std::pin::Pin;
use tokio::io::{self, AsyncWriteExt};
use tokio::task;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("ansi.moe encoder")
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
            Arg::with_name("filter_ssa_layers")
            .long("--ssa-layers")
            .takes_value(true)
            .multiple(true)
            .help("only shows subtitles on the specified layers, if using a SubStation Alpha stream.")
        )
        .arg(
            Arg::with_name("filter_ssa_styles")
            .long("--ssa-styles")
            .takes_value(true)
            .multiple(true)
            .help("only shows subtitles with the specified styles, if using a SubStation Alpha stream.")
        )
        .get_matches();

    ac_ffmpeg::set_log_callback(|_, m| {
        println!("ffmpeg: {}", m);
    });

    let input_file = File::open(matches.value_of("INPUT").unwrap()).unwrap();
    let mut demuxer = Demuxer::from_seek(input_file).unwrap();
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

    let encoder = ANSIVideoEncoder {
        stream_index: 0,
        width,
        height,
        color_mode,
        encoder_opts: EncoderOptions {
            compression_level: None,
            compression_mode: CompressionMode::None,
        },
    };

    let mut subtitle_stream_indexes = demuxer
        .subtitle_streams
        .keys()
        .map(|v| *v)
        .collect::<Vec<usize>>();

    subtitle_stream_indexes.sort();

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
                        .filter(move |v| {
                            if let SubtitlePacket::SSAEntry(entry) = v {
                                futures::future::ready(ssa_filter.check(&entry))
                            } else {
                                futures::future::ready(false)
                            }
                        }),
                )
            } else {
                demuxer
                    .subscribe_to_subtitles(subtitle_stream_indexes[subtitle_stream_idx])
                    .unwrap()
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
        color_modes: HashSet::from([encoder.color_mode]),
    };

    let video_stream = demuxer
        .subscribe_to_video(*demuxer.video_streams.keys().next().unwrap())
        .unwrap();

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

    let out_task = task::spawn(async move {
        print!("\x1B[2J\x1B[1;1H");

        let mut stdout = io::stdout();
        while let Some(val) = orx.next().await {
            stdout.write_all(&val).await?;
        }

        io::Result::Ok(())
    });

    let runner = task::spawn(player::play(resized_stream.boxed(), subtitle_stream, otx));

    tokio::try_join! {
        demuxer_task,
        runner,
        out_task
    };


    Ok(())
}
