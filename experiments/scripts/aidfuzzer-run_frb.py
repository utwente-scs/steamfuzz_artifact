#!/usr/bin/env python3
"""
Runs inside the aidfuzzer-frb docker container.
Executes FirmReBugger replay for all active experiments (aidfuzzer).

Like _collect_coverage in aidfuzzer-run_experiment.py, but without
-cov_log / -plot; output is captured to frb.log in each run directory.
"""

import argparse
import os
import signal
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

from pathlib import Path

from eval_data_processing.config import EXPERIMENTS

IOFUZZ = "./iofuzz"
TARGETS_BASE = Path("/home/user/hoedur-targets/arm")
SIMULATOR = "./simulator"
EXPERIMENTS_BASE = Path("/home/user/experiments")
FUZZER_NAME = "aidfuzzer"
IOFUZZ_WORKDIR = "/home/user/xxfuzzer/framework/bin"

VIRTUALENVWRAPPER_SH = "/usr/share/virtualenvwrapper/virtualenvwrapper.sh"
VIRTUALENV_ACTIVATE = "/home/user/.virtualenvs/fuzzware/bin/activate"


def _activate_env_prefix() -> str:
    if os.path.isfile(VIRTUALENVWRAPPER_SH):
        return f'source "{VIRTUALENVWRAPPER_SH}" && workon fuzzware'
    return f'source "{VIRTUALENV_ACTIVATE}"'


def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)


def run_frb_for_run(target, corpus_dir, timeout=None):
    """Run iofuzz in replay mode (FRB variant) for a single run directory."""
    target_dir = TARGETS_BASE / target
    config_path = target_dir / "config_aidfuzzer.yml"

    if not config_path.exists():
        eprint(f"[ERROR] config_aidfuzzer.yml not found: {config_path}")
        return

    frb_plot_log = corpus_dir / "frb_plot.log"
    cmd_parts = [
        IOFUZZ, "run",
        str(config_path),
        SIMULATOR,
        "-corpus", str(corpus_dir),
        "-plot", str(frb_plot_log),
    ]
    filter_file = target_dir / "valid_basic_blocks.txt"
    if filter_file.exists():
        cmd_parts += ["-filter", str(filter_file)]

    iofuzz_args = " ".join(str(p) for p in cmd_parts)
    bash_cmd = ["bash", "-c", f'{_activate_env_prefix()} && {iofuzz_args}']

    descriptor_path = Path("/home/user/experiments/targets/arm") / target / "bug_descriptor.c"
    symbols_path = Path("/home/user/experiments/targets/arm") / target / "symbols.txt"

    frb_log = corpus_dir / "frb.log"
    eprint(f"[*] Running FRB replay: {iofuzz_args}")
    with open(frb_log, "w") as log:
        # start_new_session=True puts iofuzz/QEMU in their own process group so
        # that, on timeout, we can kill the whole tree instead of just the
        # immediate `bash -c` child (which subprocess's own timeout handling
        # would leave running as an orphan, still stuck, forever).
        proc = subprocess.Popen(
            bash_cmd, stdout=log, stderr=log, cwd=IOFUZZ_WORKDIR,
            env={"FIRMREBUGGER_SYMBOLS": str(symbols_path), "FIRMREBUGGER_CONFIG": str(descriptor_path), **os.environ},
            start_new_session=True,
        )
        try:
            returncode = proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            eprint(f"[WARNING] FRB replay for {target} ({corpus_dir}) timed out after {timeout}s "
                   f"(likely a defective/empty corpus for this run), killing and skipping: {frb_log}")
            os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            proc.wait()
            return

    if returncode != 0:
        eprint(f"[WARNING] FRB replay exited with {returncode}, see {frb_log}")
    else:
        eprint(f"[+] FRB replay done: {frb_log}")


def process_experiment(experiment):
    name = experiment['name']
    runs = experiment['runs']
    targets = experiment['target']

    tasks = []
    for fuzzer in experiment['fuzzer']:
        if fuzzer != FUZZER_NAME:
            continue

        corpus_base = EXPERIMENTS_BASE / name / "results" / "fuzzing-runs" / FUZZER_NAME / f"{name}-{FUZZER_NAME}"

        for target in targets:
            descriptor_path = Path("/home/user/experiments/targets/arm") / target / "bug_descriptor.c"
            if not descriptor_path.exists():
                eprint(f"[*] Skipping {target}: no bug_descriptor.c (not a FirmReBugger target)")
                continue
            for run_id in range(1, runs + 1):
                run_dir = corpus_base / f"{target}_{run_id:02d}"
                if not run_dir.is_dir():
                    eprint(f"[WARNING] Run directory missing, skipping: {run_dir}")
                    continue
                eprint(f"[*] Queuing {name} / {target} run {run_id:02d}")
                tasks.append((target, run_dir))

    return tasks


def main():
    parser = argparse.ArgumentParser(description='Run FirmReBugger replay for all active AIDFuzzer experiments.')
    parser.add_argument('--timeout', type=int, default=1800,
                        help='Per-run replay timeout in seconds (default: 1800 = 30 min). '
                             'A run that exceeds this (e.g. due to a defective/empty corpus) '
                             'is killed and skipped instead of hanging the whole batch.')
    args = parser.parse_args()

    all_tasks = []
    for name, experiment in EXPERIMENTS.items():
        print(f'Collecting AIDFuzzer FirmReBugger tasks for experiment: {name}')
        all_tasks.extend(process_experiment(experiment))

    print(f'[*] Running {len(all_tasks)} tasks on 80 workers (per-run timeout: {args.timeout}s)')
    with ThreadPoolExecutor(max_workers=80) as pool:
        futures = {pool.submit(run_frb_for_run, target, run_dir, args.timeout): (target, run_dir)
                   for target, run_dir in all_tasks}
        for future in as_completed(futures):
            target, run_dir = futures[future]
            exc = future.exception()
            if exc:
                eprint(f"[ERROR] Task {target} / {run_dir} raised: {exc}")


if __name__ == '__main__':
    main()
