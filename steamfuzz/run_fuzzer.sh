#!/usr/bin/env bash

cargo run --bin hoedur-arm --release -- --name $2 --config ./setup/$1/config.yml --fuzzware --model-share ./out/models  fuzz --statistics
