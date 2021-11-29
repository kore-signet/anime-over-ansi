use anime_telnet::metadata::VideoMetadata;
use futures::StreamExt;
use play::codec::PacketReadCodec;

use clap::Arg;

use rmp_serde as rmps;
use tokio::io::AsyncReadExt;
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
    let metadata: VideoMetadata = rmps::from_read_ref(&metadata_bytes).unwrap();

    println!("\x1b[1m> video tracks\x1b[0m");
    for track in metadata.video_tracks {
        println!("\x1b[1mStream \x1b[0m{}", track.index);
        println!(
            "\x1b[1mTitle: \x1b[0m{}",
            track.name.unwrap_or("<undefined>".to_owned())
        );
        println!("\x1b[1mColor mode: \x1b[0m{}", track.color_mode);
        println!("\x1b[1mCompression: \x1b[0m{}", track.compression);
        println!("\x1b[1mDimensions: \x1b[0m{}x{}", track.width, track.height);
        println!();
    }

    println!();

    println!("\x1b[1m> subtitle tracks\x1b[0m");
    for track in metadata.subtitle_tracks {
        println!("\x1b[1mStream \x1b[0m{}", track.index);
        println!(
            "\x1b[1mTitle: \x1b[0m{}",
            track.name.unwrap_or("<undefined>".to_owned())
        );
        println!(
            "\x1b[1mLanguage: \x1b[0m{}",
            track.lang.unwrap_or("<undefined>".to_owned())
        );
        println!("\x1b[1mFormat: \x1b[0m{}", track.format);
        println!();
    }

    if matches.is_present("show_packets") {
        let mut packet_stream = FramedRead::new(input_fs, PacketReadCodec::new(true));
        let mut idx = 0;

        while let Some(packet) = packet_stream.next().await {
            let packet = packet?;
            if matches
                .values_of("show_packets")
                .unwrap()
                .map(|v| v.parse::<u32>().unwrap())
                .any(|x| x == packet.stream_index)
            {
                println!("packet {} at stream {}", idx, packet.stream_index);
                println!("presentation timestamp: {:?}", packet.time);
                if let Some(duration) = packet.duration {
                    println!("duration: {:?}", duration);
                }

                idx += 1;
            }
        }
    }

    Ok(())
}
