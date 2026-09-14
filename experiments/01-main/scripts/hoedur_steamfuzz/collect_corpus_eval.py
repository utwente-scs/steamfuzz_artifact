#!/usr/bin/env python3
"""
Parse .cov.log files from hoedur/steamfuzz fuzzing runs and extract per-corpus statistics:
  - number of inputs processed
  - average interrupts per input
  - average MMIO reads per input
  - average basic blocks executed per input

Output is stored as JSON files in:
  {outdir}/{fuzzer}/{target_flat}.json

File discovery:
  {basedir}/{fuzzer}/*-{fuzzer}/TARGET-*-FUZZER-{fuzzer}-RUN-*-*.cov.log

Log format (three lines per input):
  HH:MM:SS INFO    hoedur::coverage - Running input N ...
  HH:MM:SS INFO    hoedur::coverage - Input info: interrupts X, mmio reads Y
  HH:MM:SS INFO    hoedur::coverage - Result: OUTCOME, B basic blocks executed in T ms, ...

Steamfuzz additionally reports the number of message windows on the info line:
  HH:MM:SS INFO    hoedur::coverage - Input info: interrupts X, mmio reads Y, message windows Z
When present, an `avg_message_windows` statistic is emitted (Steamfuzz only).

For plain hoedur the result line carries the interrupt count itself:
  Result: OUTCOME, B basic blocks executed in T ms, I interrupts, read M (K input) MMIO values, irqs Q
and the value behind `irqs` is used as the interrupt count instead of the one on
the info line.
"""

import argparse
import json
import re
import sys
from pathlib import Path

RE_RUNNING_FIRST = re.compile(r'Running input 1 \.\.\.')
RE_RUNNING    = re.compile(r'Running input \d+ \.\.\.')
RE_INPUT_INFO = re.compile(
    r'Input info: interrupts (\d+), mmio reads (\d+)(?:, message windows (\d+))?')
RE_RESULT     = re.compile(r'Result: \w+, (\d+) basic blocks executed')
RE_CRASH      = re.compile(r'Result: Crash')
# hoedur only: ..., {} interrupts, read {} ({} input) MMIO values, irqs {}
RE_RESULT_IRQS = re.compile(r'Result: \w+, \d+ basic blocks executed.*?\birqs (\d+(?:\.\d+)?)')

# TARGET-{target}-FUZZER-{fuzzer}-RUN-{N}-DURATION-{D}-MODE-fuzzware.cov.log
RE_FILENAME = re.compile(
    r'^TARGET-(.+)-FUZZER-(\w+)-RUN-(\d+)-DURATION-(\w+)-MODE-fuzzware\.cov\.log$'
)


def parse_cov_log(log_path: Path, fuzzer: str = None) -> dict:
    """Parse a single .cov.log file and return per-run aggregate statistics.

    For plain hoedur (fuzzer == 'hoedur') the interrupt count is taken from the
    `irqs` value on the result line, falling back to the info line when absent.
    """
    use_irqs = fuzzer == 'hoedur'
    interrupts_list = []
    mmio_reads_list = []
    message_windows_list = []  # populated only when present (Steamfuzz)
    bbs_list = []
    num_inputs_seen = 0
    pending_info = None
    done_with_crashes = False

    with open(log_path, 'r', errors='replace') as f:
        for line in f:
            if not done_with_crashes and not RE_RUNNING_FIRST.search(line):
                continue
            done_with_crashes = True
            if RE_RUNNING.search(line):
                num_inputs_seen += 1
                pending_info = None
                continue
            m = RE_INPUT_INFO.search(line)
            if m:
                windows = int(m.group(3)) if m.group(3) is not None else None
                pending_info = (int(m.group(1)), int(m.group(2)), windows)
                continue
            m = RE_CRASH.search(line)
            if m:
                pending_info = None
                continue
            m = RE_RESULT.search(line)
            if m and pending_info is not None:
                bbs_list.append(int(m.group(1)))
                interrupts = pending_info[0]
                if use_irqs:
                    m_irqs = RE_RESULT_IRQS.search(line)
                    if m_irqs:
                        irqs = m_irqs.group(1)
                        interrupts = float(irqs) if '.' in irqs else int(irqs)
                interrupts_list.append(interrupts)
                mmio_reads_list.append(pending_info[1])
                if pending_info[2] is not None:
                    message_windows_list.append(pending_info[2])
                pending_info = None

    n = len(bbs_list)
    stats = {
        'num_inputs': num_inputs_seen,
        'avg_interrupts':    sum(interrupts_list) / n if n else None,
        'avg_mmio_reads':    sum(mmio_reads_list)  / n if n else None,
        'avg_basic_blocks':  sum(bbs_list)          / n if n else None,
    }
    # Emitted only when the log reports message windows (Steamfuzz).
    if message_windows_list:
        stats['avg_message_windows'] = sum(message_windows_list) / len(message_windows_list)
    return stats


def collect(basedir: Path, fuzzers: list, outdir: Path) -> None:
    """Discover all .cov.log files and write per-(target, fuzzer) JSON stats."""
    # results[(target_flat, fuzzer)] = {run_id: stats_dict}
    results: dict = {}

    for fuzzer in fuzzers:
        for cov_log in sorted(basedir.glob(f'{fuzzer}/*-{fuzzer}/*.cov.log')):
            m = RE_FILENAME.fullmatch(cov_log.name)
            if not m:
                print(f'WARNING: unexpected filename, skipping: {cov_log.name}',
                      file=sys.stderr)
                continue
            target_flat = m.group(1)
            file_fuzzer  = m.group(2)
            run_id       = int(m.group(3))

            key = (target_flat, file_fuzzer)
            if key not in results:
                results[key] = {}

            print(f'  Parsing {cov_log.name} ...', flush=True)
            results[key][run_id] = parse_cov_log(cov_log, file_fuzzer)

    for (target_flat, fuzzer), runs_data in sorted(results.items()):
        fuzzer_out = outdir / fuzzer
        fuzzer_out.mkdir(parents=True, exist_ok=True)
        out_file = fuzzer_out / f'{target_flat}.json'
        payload = {
            'target_flat': target_flat,
            'fuzzer':      fuzzer,
            'runs': {str(k): v for k, v in sorted(runs_data.items())},
        }
        with open(out_file, 'w') as f:
            json.dump(payload, f, indent=2)
        print(f'Wrote: {out_file}', flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(
        description='Parse hoedur/steamfuzz .cov.log files and extract corpus evaluation statistics.')
    parser.add_argument('--basedir', required=True,
                        help='Base directory containing fuzzing runs '
                             '(e.g., results/fuzzing-runs/)')
    parser.add_argument('--outdir', required=True,
                        help='Output directory for corpus eval statistics '
                             '(e.g., results/statistics/corpus_eval/)')
    parser.add_argument('--fuzzers', nargs='+',
                        default=['hoedur', 'hoedur_steamfuzz'],
                        help='Fuzzers to process (default: hoedur hoedur_steamfuzz)')
    args = parser.parse_args()

    basedir = Path(args.basedir)
    outdir  = Path(args.outdir)

    if not basedir.is_dir():
        print(f'ERROR: basedir does not exist: {basedir}', file=sys.stderr)
        sys.exit(1)

    collect(basedir, args.fuzzers, outdir)


if __name__ == '__main__':
    main()
