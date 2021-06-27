# allie's ANSI anime experience
code to encode video into ANSI escape sequences, then play it back at the proper framerate and optionally with subtitles.

## tips
```bash
# process video into image sequence to be encoded
ffmpeg -i video.mkv -vsync 0 -f image2 frame-%09d.png
# serve video over tcp
./target/release/player video.txt | nc -l 2323
```

### todos
- extract frames directly from video file
