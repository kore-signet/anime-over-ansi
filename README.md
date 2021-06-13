# video over telnet
#### some code to transmit 24fps video over telnet, using ANSI escape codes and unicode trickery

you do need to do a bit of work to wire this together.
```bash
ffmpeg -i video.mkv -vsync 0 -f image2 frame-%09d.png ## transform video into series of png frames
ls -1 *.png | parallel --bar -P 8 mogrify -resize 192x108 -filter Lanczos `cat -` # uses imagemagick to resize frames into 192x108, using Lanczos interpolation
ls -1 *.png | parallel --bar -P 8 ./dither # uses Floyd-Steinberg dithering and delta E color difference to transform the images into 8bit color ones
ls -1 *.png | parallel --bar -P 8 ./encode # encodes the images into txt files containing the actual data to be transmitted
ls -1 *.txt >> frames.txt # creates a list of all the encoded frames
./server # actually runs the server
```

color data from https://jonasjacek.github.io/colors/
