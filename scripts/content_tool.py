import argparse
import pathlib
import hashlib
import subprocess
import shutil
import uuid

'''
Structure example

content/
    .content_folder
    originals/
        city/
            city1.mp4
    encoded_db/
        0b5da870ec52dd62cfd60ebdbeb659bc88fa4218eed9f4b086cdeb73925b4155.h264 - Encoded version, named after sha hash of original
        0b5da870ec52dd62cfd60ebdbeb659bc88fa4218eed9f4b086cdeb73925b4155.png - Thumbnail, named after sha hash of original
    output/
        encoded/
            city/
                city1.h264 -> 0b5da870ec52dd62cfd60ebdbeb659bc88fa4218eed9f4b086cdeb73925b4155.h264
        thumbnails/
'''

def sha256sum(filename):
    with open(filename, 'rb', buffering=0) as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()

def walk(path): 
    for p in path.iterdir(): 
        if p.is_dir(): 
            yield from walk(p)
            continue
        yield p.resolve()

def main():
    parser = argparse.ArgumentParser(
            prog='content_tool.py',
            description='Organizes glitcher videos')

    subparsers = parser.add_subparsers(help='Command', dest='command')

    encode_parser = subparsers.add_parser('encode', help="Encode only new videos")
    encode_parser.add_argument('files', nargs='+', type=pathlib.Path, help="Originals to reencode")

    reencode_parser = subparsers.add_parser('reencode', help="(Re)encode specified videos")
    reencode_parser.add_argument('files', nargs='+', type=pathlib.Path, help="Originals to reencode")

    symlink_parser = subparsers.add_parser('symlink')
    symlink_parser.add_argument('content_folder', type=pathlib.Path, help="Content folder")

    ingest_parser = subparsers.add_parser('ingest')
    ingest_parser.add_argument('content_folder', type=pathlib.Path, help="Content folder")
    ingest_parser.add_argument('old_folder', type=pathlib.Path, help="Old structure content folder")

    args = parser.parse_args()

    if args.command == 'encode':
        encode(args.files)
    if args.command == 'reencode':
        encode(args.files, skip_if_exists=False)
    if args.command == 'symlink':
        symlink(args.content_folder)
    if args.command == 'ingest':
        ingest(args.content_folder, args.old_folder)

def get_content_folder(path):
    for parent in path.resolve().parents:
        if (parent / ".content_folder").exists():
            return parent
    raise RuntimeError("File is not contained in content folder")

def get_encoded_path(path):
    sha = sha256sum(path)
    folder = get_content_folder(path)
    return get_encoded_path_from_sha(sha, folder) 

def get_encoded_path_from_sha(sha, content_folder):
    return content_folder / "encoded_db" / (sha + ".h264")

def get_thumbnail_path(path):
    sha = sha256sum(path)
    folder = get_content_folder(path)
    return get_thumbnail_path_from_sha(sha, folder)

def get_thumbnail_path_from_sha(sha, content_folder):
    return content_folder / "encoded_db" / (sha + ".png")

def encode_single(path, skip_if_exists=True):
    encoded_path = get_encoded_path(path)
    encoded_path.parent.mkdir(exist_ok=True)

    if skip_if_exists and encoded_path.exists():
        print(f"{path.name} was already encoded, skipping")
    else:
        # Encode video
        process = subprocess.run(
            [
                "ffmpeg",
                "-y",
                '-noautorotate',
                "-i",
                str(path),
                '-c:v', 'libx264',
                '-vf', 'format=yuv420p,scale=1920:1080',
                '-qp', '30',
                '-x264-params', 'bframes=0:ref=1:min-keyint=infinite:cabac=1:scenecut=0',
                '-threads', '4',
                str(encoded_path)
            ],
            stderr=subprocess.STDOUT,
            encoding='utf-8',
        )
        if process.returncode != 0:
            print(process.stderr)


    thumbnail_path = get_thumbnail_path(path)
    if skip_if_exists and thumbnail_path.exists():
        print(f"{path.name} was already thumbnailed, skipping")
    else:
        # Encode thumbnail
        # create thumbnail from encoded, in case original is a dummy
        process = subprocess.run(
            [
                "ffmpeg",
                "-y",
                '-noautorotate',
                "-i",
                str(encoded_path),
                '-vf', 'scale=320:180',
                '-vframes', '1',
                str(thumbnail_path)
            ],
            stderr=subprocess.STDOUT,
            encoding='utf-8',
        )
        if process.returncode != 0:
            print(process.stderr)

def encode(paths, skip_if_exists=True):
    for path in paths:
        if path.is_dir():
            continue
        encode_single(path, skip_if_exists)

def symlink(content_folder):
    content_folder = content_folder.resolve()
    if not (content_folder / ".content_folder").exists():
        raise RuntimeError("This is not a content folder")

    in_dir = content_folder / "originals"
    out_dir = content_folder / "output"
    out_enc_dir = out_dir / "encoded"
    out_thumb_dir = out_dir / "thumbnails"

    # Delete exisiting symlink folder
    if out_dir.exists():
        shutil.rmtree(out_dir)

    for path in walk(in_dir):
        if path.with_suffix('').name.endswith('_rem'):
            print(f"Skipping {path.name}")
            continue
        sha, folder = sha256sum(path), get_content_folder(path)
        e = get_encoded_path_from_sha(sha, folder)
        t = get_thumbnail_path_from_sha(sha, folder)
        if e.exists() and t.exists():
            rel_path = path.relative_to(in_dir)

            out_path = out_enc_dir / rel_path.with_suffix(".h264")
            out_path.parent.mkdir(parents=True, exist_ok=True)
            out_path.symlink_to(e)

            out_path = out_thumb_dir / rel_path.with_suffix(".png")
            out_path.parent.mkdir(parents=True, exist_ok=True)
            out_path.symlink_to(t)
            print(rel_path)
        else:
            print("{path} is missing encoding or thumbnail")

def find_path_without_extension(path_without_ext):
    path_without_ext = path_without_ext.resolve()
    print(f"Looking for {path_without_ext}")
    if path_without_ext.parent.exists():
        for p in path_without_ext.parent.iterdir(): 
            if p.resolve().with_suffix('') == path_without_ext:
                print(f"found {p.resolve()}")
                return p.resolve()

def ingest(content_folder, old_folder):
    content_folder = content_folder.resolve()
    old_folder = old_folder.resolve()
    if not (content_folder / ".content_folder").exists():
        raise RuntimeError("This is not a content folder")

    orig_dir = content_folder / "originals"

    old_orig_dir = old_folder / "original"
    old_enc_dir = old_folder / "encoded"
    old_thumb_dir = old_folder / "thumbnails"

    # Copy all originals
    for orig in walk(old_orig_dir):
        new_path = orig_dir / orig.relative_to(old_orig_dir)
        orig_sha = sha256sum(orig)
        if new_path.exists():
            if sha256sum(new_path) != orig_sha:
                print(f"File {new_path} exists, but is different from {orig}")
            else:
                print(f"File {new_path} exists and is the same")
        else:
            new_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(orig, new_path)



    # Copy all encoded, creating link to original, or dummy original
    for enc in walk(old_enc_dir):
        orig_path = find_path_without_extension((orig_dir / (enc.relative_to(old_enc_dir))).with_suffix(''))
        if orig_path is None:
            orig_path = orig_dir / enc.relative_to(old_enc_dir).with_suffix(".dummy")
            print(f"{enc} has no original, creating dummy at {orig_path}")
            orig_path.parent.mkdir(parents=True, exist_ok=True)
            with open(orig_path,'w') as f:
                f.write(str(uuid.uuid4()))
            orig_sha = sha256sum(orig_path)
            print(f"dummy sha {orig_sha}")
        else:
            orig_sha = sha256sum(orig_path)


        print(f"{enc} has original {orig_path}")
        new_enc_path = get_encoded_path_from_sha(orig_sha, content_folder)
        if new_enc_path.exists():
            if sha256sum(new_enc_path) != sha256sum(enc):
                print(f"File {new_enc_path} exists, but is different from {enc}")
            else:
                print(f"File {new_enc_path} exists and is the same")
        else:
            new_enc_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(enc, new_enc_path)

        


if __name__ == "__main__":
    main()
