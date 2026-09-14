#!/bin/sh
set -ex

# reset previously applied patches
git reset --hard HEAD
git clean -fd

# apply patches
variant=""
for dir in "$@"; do
    if [ "$variant" = "" ]
    then
        variant="$dir"
    else
        variant="$variant-$dir"
    fi
done

if [ -z "$variant" ]
then
    git apply -p1 patches/hoedur.patch
    # git checkout hoedur
fi

if [ "$variant" = "steamfuzz_ablation1" ]
then
    # build hoedur variant (no mutations)
    mkdir -p /tmp/bin
    cargo install --path "$HOEDUR/hoedur" \
        --bin hoedur-arm \
        --root /tmp \
        --no-track \
        --features no-interval-mutation,no-insert-message-mutation
elif [ "$variant" = "steamfuzz_ablation2" ]
then
    # build hoedur variant (only message mutations)
    mkdir -p /tmp/bin
    cargo install --path "$HOEDUR/hoedur" \
        --bin hoedur-arm \
        --root /tmp \
        --no-track \
        --features no-interval-mutation
else
    # build hoedur variant
    mkdir -p /tmp/bin
    cargo install --path "$HOEDUR/hoedur" \
        --bin hoedur-arm \
        --root /tmp \
        --no-track
fi

if [ -z "$variant" ]
then
    mv /tmp/bin/hoedur-arm "/home/user/.cargo/bin/hoedur-arm"
else
    mv /tmp/bin/hoedur-arm "/home/user/.cargo/bin/hoedur-$variant-arm"
fi
