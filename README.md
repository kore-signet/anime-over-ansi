# ansi.moe
code to encode video into ANSI escape sequences, then play it back at the proper framerate and optionally with subtitles.
[it has a spec!](https://ansi.moe/spec.html)

## building with cuda

as Rust-GPU's Rust-CUDA libraries are extremely picky about their environment, the most reliable way i've found to build ansi.moe with CUDA support is using docker.
assuming you have docker installed and running, you can use the included `build_cuda.sh` script to build a docker image with all required dependencies (beware, this will compile ffmpeg from scratch) and subsequently build the encoder and direct_play binaries.


<!-- ## tips
```bash
# encoder syntax
./target/release/encoder \
--track=color:256,compression:zstd,width:192,height:108,name:eightbit \
--track=color:true,compression:zstd,width:192,height:108,name:colorful \
input.mkv out.ansi
``` -->