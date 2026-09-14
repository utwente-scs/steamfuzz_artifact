#!/usr/bin/env python3
"""
Runs inside the frb:Hoedur docker container.
Executes FirmReBugger for all active experiments (Hoedur / Steamfuzz fuzzers).
"""

import argparse
import multiprocessing
import os
from pathlib import Path

from eval_data_processing.config import EXPERIMENTS
from fuzz import do_run_frb


def _collect_work_items(experiment):
    name = experiment['name']
    basedir = experiment['path']
    duration = experiment['duration']
    runs = experiment['runs']
    cores = experiment.get('cores', 1)
    count = runs * cores
    targets = experiment['target']

    items = []
    for fuzzer in experiment['fuzzer']:
        if 'hoedur' not in fuzzer:
            continue

        fuzzing_dir = basedir / 'results' / 'fuzzing-runs' / fuzzer
        corpus_dir = fuzzing_dir / f'{name}-{fuzzer}'

        for target in targets:
            descriptor_path = Path("/home/user/hoedur-experiments/targets/arm") / target / "bug_descriptor.c"
            if not descriptor_path.exists():
                print(f"  Skipping {target}: no bug_descriptor.c (not a FirmReBugger target)")
                continue
            target_dashed = target.replace('/', '-')
            symbols_path = Path("/home/user/hoedur-experiments/targets/arm") / target / "symbols.txt"
            for run_id in range(count):
                base_filename = (
                    f'TARGET-{target_dashed}'
                    f'-FUZZER-{fuzzer}'
                    f'-RUN-{run_id + 1:02d}'
                    f'-DURATION-{duration}'
                    f'-MODE-fuzzware'
                )
                archive = corpus_dir / f'{base_filename}.corpus.tar.zst'
                corpus = str(corpus_dir / base_filename)
                items.append((archive, fuzzer, target, corpus, str(descriptor_path), str(symbols_path), run_id))
    return items


def run_frb_experiment(experiment):
    """Collect and return all work items for a single experiment."""
    return _collect_work_items(experiment)


def _run_single(args):
    archive, fuzzer, target, corpus, descriptor_path, symbols_path, run_id = args
    print(f"Running FRB for {target} run {run_id}, using descriptor: {descriptor_path}")
    os.environ["FIRMREBUGGER_CONFIG"] = descriptor_path
    os.environ["FIRMREBUGGER_SYMBOLS"] = symbols_path
    do_run_frb(archive, fuzzer, target, corpus, False, True)


def main():
    parser = argparse.ArgumentParser(description='Run FirmReBugger for all active experiments.')
    parser.add_argument('--cores', type=int, default=multiprocessing.cpu_count(),
                        help='Number of parallel FRB workers (default: nproc)')
    args = parser.parse_args()

    all_items = []
    for name, experiment in EXPERIMENTS.items():
        print(f'Collecting FirmReBugger work items for experiment: {name}')
        all_items.extend(_collect_work_items(experiment))

    print(f'Running {len(all_items)} FRB task(s) on {args.cores} core(s)')
    with multiprocessing.Pool(args.cores) as pool:
        pool.map(_run_single, all_items)


if __name__ == '__main__':
    main()
