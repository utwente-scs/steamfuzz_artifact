#!/bin/sh

DIR="$(dirname "$(readlink -f "$0")")"

experiment_dir=$DIR/../..
aidfuzzer_runs_dir=$experiment_dir/results/fuzzing-runs/aidfuzzer
corpus_eval_dir=$experiment_dir/results/statistics/corpus_eval

echo "[*] Collecting corpus eval statistics for AIDFuzzer ..."
python3 "$DIR/collect_corpus_eval_aidfuzzer.py" \
    --basedir="$aidfuzzer_runs_dir" \
    --outdir="$corpus_eval_dir"
