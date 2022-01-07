# Live H264 Glitcher

Creates glitch effects in H.264 encoded videos by removing keyframes, combining videos, looping intermediate frames.

Can be controlled via OSC.

## Prepare Videos

Use ffmpeg to convert the videos to a raw h264 stream
```
ffmpeg -i video.mp4 video.h264
```

It can also be helpful encode all input videos the same way, otherwise transitions between videos don't work properly.
These settings have an effect on the glitch effects look in general and could probably be optimized still.
They also have an effect on how likely mpv is to lock up when switching videos.
```
ffmpeg -i $f -c:v libx264 -vf format=yuv420p -vf scale=1920:1080 -qp 30 ../videos_conv_lq/(basename $f .mp4).h264
```

Other potentially interesting parameters:
- `-x264-params bframes=0:refs=1` Disable B-frames and allow max 1 reference frames for p-frames

## Run glitcher
```
cargo run --release -- -i videos/* | mpv --untimed --no-cache -
```

By default the glitcher listens on port 8000 for OSC messages.

## OSC messages

- `/fps <float>` Set frames per second.
- `/record_loop <bool>` Loop recording start when true is sent and stops when false is sent. After recording the loop immediately starts playing.
- `/clear_loop <bool>`  Loop is cleared as long as true is sent.
- `/pass_iframes <bool>` Lets I-frames through, disabling the glitch effect.
- `/video_num <int>` Loads the n-th video in the list of videos given to `--input`.
