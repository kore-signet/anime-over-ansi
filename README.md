# ansi.moe
code to encode video into ANSI escape sequences, then play it back at the proper framerate and optionally with subtitles.
[it has a spec!](https://ansi.moe/spec.html)

## note - this is the experiments branch!
the current experiment is adding multi-track container support to the output files.
the player currently does not support this and thus is broken in this branch, and the encoder is a work-in-progress.

## tips
```bash
# encoder syntax
./target/release/encoder \
--track=color:256,compression:zstd,width:192,height:108,name:eightbit \
--track=color:true,compression:zstd,width:192,height:108,name:colorful \
input.mkv out.ansi
```

### todos
- live encoder
- live subtitles