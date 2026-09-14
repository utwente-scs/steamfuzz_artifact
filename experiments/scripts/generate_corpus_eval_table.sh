#!/bin/sh
# Generate corpus evaluation statistics and produce the LaTeX table.
#
# Runs for the experiment selected by the active run-time profile
# (experiment-config/active_profile.txt).
#
# Steps:
#   1. Collect per-input statistics from .cov.log files for hoedur/steamfuzz runs.
#   2. Collect AIDFuzzer corpus eval statistics (boilerplate – not yet implemented).
#   3. Generate the LaTeX table comparing Hoedur, Steamfuzz, and AIDFuzzer.

DIR="$(dirname "$(readlink -f "$0")")"
experiments_dir=$DIR/..

experiment=$(python3 -c "import sys; sys.path.insert(0, '$DIR/eval_data_processing'); from config import active_experiment; print(active_experiment()['name'])") || exit 1
experiment_dir=$experiments_dir/$experiment

echo "[*] Active experiment: $experiment"

# 1. Collect hoedur / steamfuzz corpus eval statistics
echo "[1/3] Collecting hoedur/steamfuzz corpus eval statistics ..."
sh "$experiment_dir/scripts/hoedur_steamfuzz/hoedur_steamfuzz_collect_corpus_eval.sh"

# 2. Collect AIDFuzzer corpus eval statistics (not yet implemented)
echo "[2/3] Collecting AIDFuzzer corpus eval statistics ..."
sh "$experiment_dir/scripts/aidfuzzer/aidfuzzer_collect_corpus_eval.sh"

# 3. Generate the LaTeX table
echo "[3/3] Generating LaTeX corpus eval table ..."
python3 "$DIR/eval_data_processing/print_table_corpus_eval.py"
