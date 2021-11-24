use anime_telnet::encoding::EncodedPacket;
use cyanotype::SubtitlePacket;

use futures::stream::{Stream, StreamExt};
use std::collections::VecDeque;
use std::pin::Pin;
use tokio::io::{self, AsyncWriteExt};

use tokio::time::{sleep, Instant};

pub async fn play(
    mut video_stream: Pin<Box<dyn Stream<Item = EncodedPacket> + Send>>,
    subtitle_stream: Pin<Box<dyn Stream<Item = SubtitlePacket> + Send>>,
) -> io::Result<()> {
    print!("\x1B[2J\x1B[1;1H");

    let mut subtitle_stream = subtitle_stream.ready_chunks(128);
    let mut subtitle_buffer = VecDeque::new();
    let play_start = Instant::now();
    let mut stdout = io::stdout();

    loop {
        tokio::select! {
            Some(video_frame) = video_stream.next() => {
                subtitle_buffer.retain(|v| {
                     if let SubtitlePacket::SRTEntry(e) = v {
                         e.start > video_frame.time
                     } else if let SubtitlePacket::SSAEntry(e) = v {
                         e.style.as_ref().unwrap() == "Default" && e.start.unwrap() > video_frame.time
                     } else {
                         false
                    }
                });

                if let Some(duration) = video_frame.time.checked_sub(play_start.elapsed()) {
                    sleep(duration).await;
                }

                stdout.write_all(b"\x1B[1;1H").await?;
                stdout.write_all(&video_frame.data).await?;
                stdout.write_all(b"\x1B[0m").await?;
                if let Some(SubtitlePacket::SRTEntry(e)) = subtitle_buffer.front() {
                    stdout.write_all(b"\x1B[0J ").await?;
                    stdout.write_all(e.text.as_bytes()).await?;
                } else if let Some(SubtitlePacket::SSAEntry(e)) = subtitle_buffer.front() {
                    stdout.write_all(b"\x1B[0J ").await?;
                    let s = substation::parser::text_line(&e.text).unwrap().1.into_iter().filter_map(|v| {
                        if let substation::TextSection::Text(s) = v {
                            Some(s)
                        } else {
                            None
                        }
                    }).collect::<Vec<String>>().join("").into_bytes();
                    stdout.write_all(&s).await?;
                }
            },
            Some(subtitle_chunk) = subtitle_stream.next() => {
                subtitle_buffer.extend(subtitle_chunk);
            },
            else => break
        }
    }

    Ok(())
}
