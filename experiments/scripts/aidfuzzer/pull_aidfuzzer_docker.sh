#!/bin/sh
DIR="$(dirname "$(readlink -f "$0")")"
AIDDIR="$DIR/aidfuzzer"

rm -rf $AIDDIR
git clone https://github.com/wjqsec/aidfuzzer.git $AIDDIR
git -C $AIDDIR checkout 007aa2f01131c6106e635103d165fdc47839af77

docker build \
  --build-arg USER_ID=$(id -u) \
  --build-arg GROUP_ID=$(id -g) \
  -t aidfuzzer "$DIR"
