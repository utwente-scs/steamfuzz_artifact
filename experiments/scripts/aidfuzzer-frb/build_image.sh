#!/bin/sh

DIR="$(dirname "$(readlink -f "$0")")"

docker build --no-cache -t aidfuzzer-frb:latest $DIR -f $DIR/Dockerfile