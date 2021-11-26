# ansi.moe
code to encode video into ANSI escape sequences, then play it back at the proper framerate and optionally with subtitles.
[it has a spec!](https://ansi.moe/spec.html)

## tips
```bash
# encoder syntax
./target/release/encoder \
--track=color:256,compression:zstd,width:192,height:108,name:eightbit \
--track=color:true,compression:zstd,width:192,height:108,name:colorful \
input.mkv out.ansi
```