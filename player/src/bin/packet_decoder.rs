use futures::{Stream, StreamExt};
use player::*;
use tokio::io::{self, AsyncWriteExt};
use tokio::time::{sleep_until, Instant};
use tokio_util::codec::FramedRead;

#[tokio::main]
async fn main() {
    let mut packet_stream = FramedRead::new(
        tokio::fs::File::open("out.ansi").await.unwrap(),
        PacketDecoder::new(),
    );

    let mut stdout = io::stdout();
    let play_start = Instant::now();
    while let Some(packet) = packet_stream.next().await {
        let packet = packet.unwrap();
        sleep_until(play_start + packet.time).await;
        stdout.write_all(b"\x1B[1;1H").await.unwrap();
        stdout.write_all(&packet.data).await.unwrap();
        stdout.flush().await.unwrap();
    }
}
