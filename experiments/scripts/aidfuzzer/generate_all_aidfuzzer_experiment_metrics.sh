#!/bin/sh
DIR="$(dirname "$(readlink -f "$0")")"

# This script runs all AIDFuzz metric generation scripts across all experiments.
# It should be invoked via ./scripts/aidfuzzer/run_aidfuzzer_docker.sh, but each
# sub-script already invokes run_aidfuzzer_docker.sh internally, so it can also
# be run directly on the host.

for scr in "$DIR"/../../0*/scripts/aidfuzzer/aidfuzzer_collect_*; do
    echo "[*] Running $scr"
    sh "$scr"
done
