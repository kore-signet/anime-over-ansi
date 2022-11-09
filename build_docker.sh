#!/bin/bash
#apt-get update && apt-get install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libavdevice-dev libswscale-dev -y
cd /root/rust-cuda
rustup toolchain install nightly-2021-12-04 --component rust-src --component rustc-dev --component llvm-tools-preview
cargo +nightly-2021-12-04 build --bin encode --release --bin direct_play --features cuda
