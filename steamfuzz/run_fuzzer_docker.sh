#!/usr/bin/env bash

cargo run --manifest-path ./shared_copied/Cargo.toml --bin hoedur-arm --release -- --name cve3321 --config ./copied/CVE-2021-3321/config.yml fuzz
