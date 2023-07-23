# This is a doit script (pydoit.org)
# It re-encodes the mp4 videos in the directory `original` to raw h264 streams in the folder `encoded`
# and generates video thumbnails in the folder `thumbnails`

from pathlib import Path
from doit.tools import config_changed

ffmpeg = 'ffmpeg'
threads = '8'

ffmpeg_config = {
    'preoptions': [
        '-noautorotate',
    ],
    'options': [
        '-c:v', 'libx264',
        '-vf', 'format=yuv420p,scale=1920:1080',
        '-qp', '30',
        '-x264-params', 'bframes=0:ref=1:min-keyint=infinite:cabac=1:scenecut=0'
    ]
}

thumbnail_config = {
    'options': [
        '-vf', 'scale=320:320:force_original_aspect_ratio=decrease',
        '-vframes', '1'
    ]
}

def dir_creator(path):
    def create_dir():
        path.parent.mkdir(parents=True, exist_ok=True)
    return create_dir

def task_encode():
    input_directory = Path('./original/')
    output_directory = Path('./encoded/')
    for source_file in input_directory.glob('**/*.mp4'):
        encoded_file = output_directory / source_file.relative_to(input_directory).with_suffix('.h264')
        yield {
            'name': encoded_file.name,
            'actions': [
                dir_creator(encoded_file),
                [ffmpeg, '-y'] + ffmpeg_config['preoptions'] + ['-i', source_file] + ffmpeg_config['options'] + ['-threads', threads, encoded_file]
            ],
            'file_dep': [source_file],
            'targets': [encoded_file],
            'uptodate': [config_changed(ffmpeg_config)],
        }

def task_thumbnail():
    input_directory = Path('./original/')
    output_directory = Path('./thumbnails/')
    for source_file in input_directory.glob('**/*.mp4'):
        encoded_file = output_directory / source_file.relative_to(input_directory).with_suffix('.png')
        yield {
            'name': encoded_file.name,
            'actions': [
                dir_creator(encoded_file),
                [ffmpeg, '-y', '-i', source_file] + thumbnail_config['options'] + [encoded_file]
            ],
            'file_dep': [source_file],
            'targets': [encoded_file],
            'uptodate': [config_changed(thumbnail_config)],
        }
