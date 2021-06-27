# allie's ANSI anime experience
code to encode video into ANSI escape sequences, then play it back at the proper framerate and optionally with subtitles.

## tips
```bash
# serve video over tcp
./target/release/player video.txt | nc -l 2323
```

### todos
- improve frame extraction
