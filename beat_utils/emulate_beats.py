#!/usr/bin/env python3
# Script by Alexeyan

# needs pip install python-rtmidi mido python-osc
from pythonosc import udp_client
import time
import sys

oscip = "127.0.0.1"
oscport = 8000
onbeat = False # Send a message on every beat (vs on every 1/96th of a beat)
debug = True
bpm = 120

if len(sys.argv) > 1:
    oscip = sys.argv[1]
    if len(sys.argv) > 2:
        oscport = int(sys.argv[2])

print(f"Sending messages to {oscip}:{oscport}")

client = udp_client.SimpleUDPClient(oscip, oscport)

while True:
    if debug:
        print('beat')
    client.send_message("/beat", 1)
    time.sleep(1.0/(bpm/60))
