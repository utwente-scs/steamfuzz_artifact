#!/bin/bash

cd "$(dirname "$0")" || exit 1

docker run --rm --env "HOME=/home/user" \
    --mount src="$PWD/../..",target=/home/user/hoedur-experiments,type=bind \
    --mount src="$PWD/../../targets",target=/home/user/hoedur-targets,type=bind \
    hoedur-frb "$@"
