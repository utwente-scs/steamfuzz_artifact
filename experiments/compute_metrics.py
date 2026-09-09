#!/usr/bin/env python3

import os
import subprocess

from pathlib import Path

DIR = Path(os.path.dirname(os.path.realpath(__file__)))

# Check data (report-only: missing/corrupt files are logged but must not
# block metric generation / plotting for the data that IS available)
subprocess.run([DIR.joinpath("scripts", "run_in_docker.sh"), Path("scripts", "check_data.py")])

# Fuzzware metrics
subprocess.run([DIR.joinpath("scripts", "fuzzware", "run_fuzzware_docker.sh"), Path("scripts", "fuzzware", "generate_all_fuzzware_experiment_metrics.sh")])

# Hoedur metrics
subprocess.run([DIR.joinpath("scripts", "run_in_docker.sh"), Path("scripts", "hoedur-eval.py")])

# AIDFuzz metrics
subprocess.run([DIR.joinpath("scripts", "aidfuzzer", "run_aidfuzzer_docker.sh"), "sh", "/home/user/experiments/scripts/aidfuzzer/generate_all_aidfuzzer_experiment_metrics.sh"])

# Overall metrics
# 1. Summarize bug discovery timings (for every experiment that has the script)
for script in sorted(DIR.glob("0*/scripts/summarize_bug_discovery_timings.sh")):
    subprocess.run([script])
# 2. Generate tables, figures, and survivability plot (runs in hoedur-plotting-env docker)
#    Includes:  sota_comparison, ablation_study, survivability (frb.log → CDF chart + timings JSON)
#    Note: survivability requires run_firmrebugger.py to have been run first.
subprocess.run(["make", "-C", DIR.joinpath("scripts", "eval_data_processing")])
# 3. Generate bug discovery timings LaTex tables
# subprocess.run([DIR.joinpath("scripts", "eval_data_processing", "print_table_discovery_timings.py")])
# subprocess.run([DIR.joinpath("scripts", "eval_data_processing", "print_table_discovery_timings_pdf.sh")])
# 4. Generate corpus eval LaTeX table (collect .cov.log stats + render table)
subprocess.run([DIR.joinpath("scripts", "run_in_docker.sh"), Path("scripts", "generate_corpus_eval_table.sh")])
