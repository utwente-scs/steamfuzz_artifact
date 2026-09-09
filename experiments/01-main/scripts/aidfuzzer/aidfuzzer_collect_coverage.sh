#!/bin/sh

DIR="$(dirname "$(readlink -f "$0")")"

experiment_dir=$DIR/../..
aidfuzzer_scripts_dir=$experiment_dir/../scripts/aidfuzzer
aidfuzzer_runs_dir=$experiment_dir/results/fuzzing-runs/aidfuzzer
coverage_results_dir=$experiment_dir/results/coverage

targets_base=$experiment_dir/../targets/arm

python3 $aidfuzzer_scripts_dir/gather_coverage.py \
    --basedir=$aidfuzzer_runs_dir \
    --outdir=$coverage_results_dir
