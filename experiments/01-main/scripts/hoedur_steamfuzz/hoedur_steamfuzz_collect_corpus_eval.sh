#!/bin/sh

DIR="$(dirname "$(readlink -f "$0")")"

experiment_dir=$DIR/../..
fuzzing_runs_dir=$experiment_dir/results/fuzzing-runs
corpus_eval_dir=$experiment_dir/results/statistics/corpus_eval

echo "[*] Collecting corpus eval statistics for hoedur and hoedur_steamfuzz ..."
python3 "$DIR/collect_corpus_eval.py" \
    --basedir="$fuzzing_runs_dir" \
    --outdir="$corpus_eval_dir" \
    --fuzzers hoedur hoedur_steamfuzz_ablation2
