#!/usr/bin/env python3
"""
Run FirmReBugger for all active experiments (Hoedur / Steamfuzz fuzzers).

Starts the frb:Hoedur docker container via scripts/hoedur/run_docker_frb.sh
and passes it the inner script (scripts/hoedur-run_frb.py) to execute.
"""

import argparse
import os
import subprocess

from pathlib import Path

DIR = Path(os.path.dirname(os.path.realpath(__file__)))

HOEDUR_DOCKER_SCRIPT = DIR / 'scripts' / 'hoedur-frb' / 'run_docker_frb.sh'
HOEDUR_INNER_SCRIPT = 'scripts/hoedur-run_frb.py'

AIDFUZZER_DOCKER_SCRIPT = DIR / 'scripts' / 'aidfuzzer-frb' / 'run_docker_frb.sh'
AIDFUZZER_INNER_SCRIPT = 'scripts/aidfuzzer-run_frb.py'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Run FirmReBugger for all active experiments.')
    parser.add_argument('--cores', type=int, default=os.cpu_count(),
                        help='Number of parallel FRB workers (default: nproc)')
    parser.add_argument('--timeout', type=int, default=300,
                        help='Per-run FRB replay timeout in seconds, passed through to the '
                             'AIDFuzzer inner script (default: 1800 = 30 min). Prevents a '
                             'single defective/empty-corpus run from hanging the whole batch.')
    args = parser.parse_args()

    # # Hoedur / Steamfuzz
    subprocess.run([
        HOEDUR_DOCKER_SCRIPT, 'python3', f'/home/user/hoedur-experiments/{HOEDUR_INNER_SCRIPT}',
        '--cores', str(args.cores),
    ])

    # AIDFuzzer
    subprocess.run([
        AIDFUZZER_DOCKER_SCRIPT, 'python3', f'/home/user/experiments/{AIDFUZZER_INNER_SCRIPT}',
        '--timeout', str(args.timeout),
    ])

