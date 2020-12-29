{ stdenv, fetchFromGitHub, libtool, autoreconfHook, autoconf, ffmpeg }:
stdenv.mkDerivation rec {
  version = "master";
  name = "h264bitstream-${version}";

  src = fetchFromGitHub {
    owner = "aizvorski";
    repo = "h264bitstream";
    rev = "34f3c58afa3c47b6cf0a49308a68cbf89c5e0bff";
    sha256 = "0rrhzckz2a89q0chw2bfl4g89yiv9a0dcqcj80lcpdr3a1ix8q85";
  };

  nativeBuildInputs = [ libtool autoconf autoreconfHook ];
  buildInputs = [ ffmpeg ];
}

