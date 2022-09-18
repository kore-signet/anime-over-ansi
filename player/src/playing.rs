use bytes::Bytes;

use container::packet::Packet;

use futures::{Stream, StreamExt};
use std::time::Duration;
use tokio::{io::AsyncWriteExt, pin, time::Instant};

use postage::{sink::Sink, stream::Stream as PostageStream, watch};

pub async fn play(
    mut video: impl Stream<Item = Packet<Bytes>> + Unpin + Send + 'static,
    mut subs: impl Stream<Item = Packet<Bytes>> + Unpin + Send + 'static,
) -> std::io::Result<()> {
    let (mut subtitle_tx, mut subtitle_rx) = watch::channel_with_option::<Packet<Bytes>>();
    let (mut video_tx, mut video_rx) = watch::channel_with_option::<Packet<Bytes>>();

    let start = Instant::now();
    tokio::task::spawn(async move {
        let timer = tokio::time::sleep(Duration::from_millis(0));
        pin!(timer);

        while let Some(packet) = video.next().await {
            timer.as_mut().reset(start + packet.presentation_time);
            timer.as_mut().await;
            video_tx.send(Some(packet)).await;
        }
    });

    tokio::task::spawn(async move {
        let timer = tokio::time::sleep(Duration::from_millis(0));
        pin!(timer);

        while let Some(packet) = subs.next().await {
            let mut timer = timer.as_mut();
            let start = start + packet.presentation_time;
            let end = start + packet.presentation_length;
            timer.as_mut().reset(start);
            timer.as_mut().await;
            subtitle_tx.send(Some(packet)).await;
            timer.as_mut().reset(end);
            timer.as_mut().await;
            subtitle_tx.send(None).await;
        }
    });

    let mut stdout = tokio::io::stdout();
    stdout.write_all(b"\x1b[1;1H\x1b[?25l").await?;
    loop {
        tokio::select! {
            biased;

            Some(Some(v)) = video_rx.recv() => {
                stdout.write_all(b"\x1b[0m\x1b[1;1H").await?;
                stdout.write_all(&v.data).await?;
                stdout.write_all(b"\x1b[0m\n").await?;
                stdout.flush().await?;
            }
            Some(v) = subtitle_rx.recv() => {
                stdout.write_all(b"\x1b[s\x1b[0m\x1b[0J ").await?;

                if let Some(sub) = v {
                    stdout.write_all(&sub.data).await?;
                }

                stdout.write_all(b"\x1b[u").await?;

                stdout.flush().await?;
            }
            // _ =>
        }
    }

    Ok(())
}
