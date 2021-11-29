use anime_telnet::{
    encoding::PacketDecoder,
    metadata::{SubtitleFormat, VideoMetadata},
    subtitles::SSAFilter,
};
use play::{
    codec::PacketReadCodec,
    player,
    subtitles::{SRTDecoder, SSADecoder, SubtitlePacket},
};

use clap::Arg;
use dialoguer::{theme::ColorfulTheme, Select};
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use rmp_serde as rmps;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpListener;
use tokio::task::{self, JoinHandle};
use tokio_util::codec::FramedRead;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("ansi.moe player")
        .version("1.0")
        .author("allie signet <allie@cat-girl.gay>")
        .about("plays video from .ansi video container")
        .arg(
            Arg::with_name("INPUT")
                .help("file to read from")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("bind")
                .long("--bind")
                .takes_value(true)
                .help("bind a TCP server to specified address instead of outputting to stdout"),
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
                        "{} ({}x{}, color mode {}, compression: {})",
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

    let subtitle_track = if metadata.subtitle_tracks.is_empty() {
        None
    } else {
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

        if subtitle_selection < metadata.subtitle_tracks.len() {
            Some(metadata.subtitle_tracks.remove(subtitle_selection))
        } else {
            None
        }
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
                    Some(ssa_filter),
                ))),
                SubtitleFormat::SubRip => Some(Box::new(SRTDecoder::new())),
                _ => None,
            }
        } else {
            None
        };

    let mut packet_stream = FramedRead::new(input_fs, PacketReadCodec::new(true));
    let (mut stx, srx) = mpsc::channel(256);
    let (mut vtx, vrx) = mpsc::channel(256);
    if !has_subtitle_track {
        stx.close().await.unwrap();
    }

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

    let runner = task::spawn(player::play(Box::pin(vrx), Box::pin(srx), otx));

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

    let r = tokio::try_join! {
        runner,
        output_task
    };

    // beauty
    r.map(|v| {
        v.0.unwrap();
        v.1.unwrap();
    })
    .unwrap();

    Ok(())
}
