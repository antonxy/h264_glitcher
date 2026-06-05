#!/usr/bin/env bash
trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT
xset -dpms
xset s off
dunstctl set-paused true
systemctl --user stop redshift.service
#aubio-beat-osc beat -c 127.0.0.1 8000 /traktor/beat -v &
#cargo run --release --bin h264_glitcher -- --input-dir /home/anton/projects/glitcher/content_tool/content/output --listen-addr 127.0.0.1:8000 --thumbnail-server-listen-addr [::]:3000 --thumbnail-server-base-url http://127.0.0.1:3000/ | mpv --no-correct-pts --container-fps-override=1000 --no-cache - &
cargo run --release --bin h264_glitcher -- --input-dir /home/anton/projects/glitcher/content_tool/content/output --listen-addr 127.0.0.1:8000 --thumbnail-server-listen-addr [::]:3000 --thumbnail-server-base-url http://10.42.0.1:3000/ --mask mask_idx.png --push2 &
open-stage-control -l open_stage_control/h264_glitcher_Session.json -p 8080 -s 127.0.0.1:8000 &
wait
