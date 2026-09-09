#!/bin/sh
DIR="$(dirname "$(readlink -f "$0")")"
experiments_rootdir=$DIR/../../

# interactive shell
docker_options=""
if [ -t 0 ]; then
    docker_options="-t"
fi

docker run \
    --rm -i \
    $docker_options \
    --user "$(id -u):$(id -g)" \
    -e HOME=/home/user \
    -w /home/user/xxfuzzer/framework/bin \
    --mount type=bind,source="$(realpath "$experiments_rootdir")",target=/home/user/experiments \
    "aidfuzzer" "$@"
