use crate::subtitles::SubtitlePacket;
use anime_telnet::encoding::EncodedPacket;

use async_broadcast::{Receiver, Sender};
use futures::stream::{Stream, StreamExt};
use std::collections::VecDeque;
use tokio::io::{self, AsyncWriteExt, BufWriter};
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::time::{sleep, Instant};

/// Plays video into a broadcast channel, timing each frame.
pub async fn play<T, S>(
    mut video_stream: T,
    mut subtitle_stream: S,
    output: Sender<Vec<u8>>,
) -> Result<(), async_broadcast::SendError<Vec<u8>>>
where
    T: Stream<Item = EncodedPacket> + Send + Unpin,
    S: Stream<Item = SubtitlePacket> + Send + Unpin,
{
    let mut subtitle_buffer: VecDeque<SubtitlePacket> = VecDeque::new();
    let play_start = Instant::now();

    loop {
        tokio::select! {
            Some(mut video_frame) = video_stream.next() => {
                subtitle_buffer.retain(|v| v.end >= video_frame.time);

                if let Some(duration) = video_frame.time.checked_sub(play_start.elapsed()) {
                    sleep(duration).await;
                }

               // clean term; send frame; clear ansi styling
                let mut frame: Vec<u8> = Vec::new();
                frame.extend(b"\x1B[1;1H");
                frame.append(&mut video_frame.data);
                frame.extend(b"\x1B[0m");

                // show subtitle if available
                if let Some(SubtitlePacket { start, payload, .. }) = subtitle_buffer.front() {
                    if start <= &video_frame.time {
                        frame.extend(payload);
                    }
                } else {
                    frame.extend(b"\x1B[2K ");
                }

                output.broadcast(frame).await?;
            },
            Some(subtitle_chunk) = subtitle_stream.next() => {
                subtitle_buffer.push_back(subtitle_chunk);
            },
            else => break
        }
    }

    Ok(())
}

pub async fn play_to_stdout(mut orx: Receiver<Vec<u8>>) -> io::Result<()> {
    let _cleanup = crate::TerminalCleanup;
    print!("\x1b[?25l\x1B[2J\x1B[1;1H");

    let mut stdout = io::stdout();
    while let Some(val) = orx.next().await {
        stdout.write_all(&val).await?;
    }

    Ok(())
}

pub async fn play_to_tcp(mut orx: Receiver<Vec<u8>>, addr: impl ToSocketAddrs) -> io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let mut sockets = Vec::new();
    let mut to_rm = Vec::new();

    loop {
        tokio::select! {
            Ok((mut socket,addr)) = listener.accept() => {
                if socket.write_all(b"\x1B[2J\x1B[1;1H").await.is_ok() {
                    sockets.push(BufWriter::new(socket));
                    eprintln!("got new connection from {}", addr);
                    eprintln!("total connections: {}", sockets.len());
                };
            },
            Ok(msg) = orx.recv() => {
                if !to_rm.is_empty() {
                    eprintln!("disconnecting {} broken socket(s)", to_rm.len());
                }

                for i in to_rm.drain(..) {
                    if let Err(e) = sockets.remove(i).into_inner().shutdown().await {
                        eprintln!("error shutting down socket: {}",e);
                    };
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
}
