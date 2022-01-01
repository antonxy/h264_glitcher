#!/usr/bin/env bash

set -euo pipefail

set -x

cd "$(dirname "${BASH_SOURCE[0]}")"

export CFLAGS="-fPIC"

pushd deps/h264bitstream
set +e
make distclean
set -e
autoreconf -ivf
./configure --disable-shared --enable-static --prefix=/
make -j$(nproc)
rm -rf out
mkdir out
make install DESTDIR="$(readlink -e out)"
popd

echo "success, now cargo build should work"