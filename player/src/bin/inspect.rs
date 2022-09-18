use clap::clap_app;
use container::{
    codec::PacketDecoder,
    metadata::{CompressionMode, VideoMetadata},
    packet::PacketMapper,
    ValuePair,
};

use futures::StreamExt;
use player::PacketDecompressor;

use tokio::{fs::File, io::AsyncReadExt};

use tokio_util::codec::FramedRead;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = clap_app!(encoder =>
        (version: "1.0")
        (author: "emily signet <emily@cat-girl.gay>")
        (@arg INPUT: +required "input file or url for the encoder")
        (@arg SHOW_FULL_HEADER: --header "show metadata header")
        (@arg STREAM_SUMMARY: --summary "show summary of streams in file")
        (@arg SHOW_PACKETS: --packets "show individual packets")
        (@arg SHOW_DATA: --data "show inner packet data")
        (@arg SHOW_CODEC_PRIVATE: --codecextra "show codec extra data")
        (@arg STREAMS: -s --stream ... +takes_value "specific streams to show packets for (all if not specified)")
    )
    .get_matches();

    let mut input = File::open(matches.value_of("INPUT").unwrap()).await?;

    let meta_len = input.read_u64_le().await?;
    let mut metadata = vec![0u8; meta_len as usize];
    input.read_exact(&mut metadata).await?;

    let metadata: VideoMetadata = rmp_serde::from_slice(&metadata).unwrap();

    if matches.is_present("SHOW_HEADER") {
        println!("{:#?}", metadata);
    }

    if matches.is_present("STREAM_SUMMARY") {
        for v in metadata.video_tracks {
            println!(
                "stream #{} - {} ({}x{}) - color {}",
                v.index,
                v.name.clone().unwrap_or("unknown".to_owned()),
                v.width,
                v.height,
                v.color_mode
            );

            if matches.is_present("SHOW_CODEC_PRIVATE") {
                if let Some(data) = v.codec_private {
                    println!("codec private:\n{}", String::from_utf8_lossy(&data));
                }
            }
        }

        for v in metadata.subtitle_tracks {
            println!(
                "stream #{} - {}",
                v.index,
                v.name.clone().unwrap_or("unknown".to_owned()),
            );

            if matches.is_present("SHOW_CODEC_PRIVATE") {
                if let Some(data) = v.codec_private {
                    println!("codec private:\n{}", String::from_utf8_lossy(&data));
                }
            }
        }
    }

    let show_data = matches.is_present("SHOW_DATA");
    let streams_to_show = matches
        .values_of("STREAMS")
        .map(|v| {
            v.into_iter()
                .filter_map(|s| s.parse::<u16>().ok())
                .collect::<Vec<u16>>()
        })
        .unwrap_or_default();

    if !matches.is_present("SHOW_PACKETS") && !matches.is_present("SHOW_DATA") {
        return Ok(());
    }

    let mapper: Box<dyn PacketMapper> = if metadata.compression == CompressionMode::Zstd {
        #[cfg(feature = "compression")]
        {
            Box::new(PacketDecompressor::new()?)
        }

        #[cfg(not(feature = "compression"))]
        {
            Box::new(())
        }
    } else {
        Box::new(())
    };

    let mut framed_read = FramedRead::new(input, PacketDecoder::with_mapper(mapper));

    let mut idx: u64 = 0;

    while let Some(packet) = framed_read.next().await {
        let packet = packet?;
        if streams_to_show.is_empty() || streams_to_show.contains(&packet.stream_index) {
            println!(
                "STREAM {stream_index} packet #{packet_index}",
                stream_index = packet.stream_index,
                packet_index = idx
            );
            println!("presentation time: {:?}", packet.presentation_time);
            println!("presentation length: {:?}", packet.presentation_length);
            println!("data length: {}", packet.data.len());
            println!("extra data map:");
            for ValuePair { key, value } in packet.extra_data.inner.iter() {
                println!(
                    "K {} (utf8 '{}') - V {} (binary {:b})",
                    key,
                    String::from_utf8_lossy(&key.value().to_ne_bytes()),
                    value,
                    value
                );
            }

            if show_data {
                println!("{}", String::from_utf8_lossy(&packet.data));
            }
        }

        idx += 1;
    }

    Ok(())
}
