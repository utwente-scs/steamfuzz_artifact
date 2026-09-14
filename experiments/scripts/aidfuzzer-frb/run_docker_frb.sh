#!/bin/bash

cd "$(dirname "$0")" || exit 1

docker run --rm --env "HOME=/home/user" \
    --user "$(id -u):$(id -g)" \
    -w /home/user/xxfuzzer/framework/bin \
    --mount src="$PWD/../..",target=/home/user/experiments,type=bind \
    --mount src="$PWD/../../targets",target=/home/user/hoedur-targets,type=bind \
    aidfuzzer-frb:latest "$@"
