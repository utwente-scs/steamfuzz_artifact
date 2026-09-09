#!/bin/bash

cd "$(dirname "$0")" || exit 1

INSTALL="$PWD/../install"

echo Importing fuzzware docker...
zstd -dk --stdout "$INSTALL/fuzzware.docker.tar.zst" | docker load
