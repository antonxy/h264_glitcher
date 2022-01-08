# Live H264 Glitcher

Creates glitch effects in H.264 encoded videos by removing keyframes, combining videos, looping intermediate frames.

Can be controlled via OSC.


https://user-images.githubusercontent.com/4194320/148616193-57d7ca6b-afb8-4751-b33e-0c5017b6872c.mp4


## Prepare Videos

Use ffmpeg to convert the videos to a raw h264 stream
```
ffmpeg -i video.mp4 video.h264
```

It can also be helpful encode all input videos the same way, otherwise transitions between videos don't work properly.
These settings have an effect on the glitch effects look in general and could probably be optimized still.
They also have an effect on how likely mpv is to lock up when switching videos.
```
ffmpeg -i video.mp4 -c:v libx264 -vf format=yuv420p,scale=1920:1080 -qp 30 -x264-params bframes=0:refs=1:g=9999999 video.h264
```

[libx264 options](https://code.videolan.org/videolan/x264/-/blob/19856cc41ad11e434549fb3cc6a019e645ce1efe/common/base.c#L952)
Potentially interesting parameters:
- `bframes=0` Disable B-frames
- `refs=1` allow max 1 reference frames for p-frames
- `g=9999999` No keyframes inbetween

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

## Ideas

- Fluid speed control, turntable style
- Video selection UI with thumbnails, groups of videos
- Recurse directories as input

### Todos

- Find out why video sometimes stutters when switching
