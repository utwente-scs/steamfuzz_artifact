#!/bin/bash

cd "$(dirname $0)" || exit 1

env/run.sh pdflatex /home/user/hoedur-experiments/01-bug-finding-ability/results/table_1_cve_discovery_timings.tex || echo "failed table 1"
env/run.sh pdflatex /home/user/hoedur-experiments/01-bug-finding-ability/results/table_2_add_bugs_discovery_timings.tex || echo "failed table 2"

mv *.pdf ../../01-bug-finding-ability/results/