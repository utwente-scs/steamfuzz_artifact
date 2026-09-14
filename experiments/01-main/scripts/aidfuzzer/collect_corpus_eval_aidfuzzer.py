#!/usr/bin/env python3
"""
Parse iofuzz_cov.log files from AIDFuzzer fuzzing runs and extract per-corpus statistics:
  - number of inputs processed
  - average IRQs injected per input  (mapped to 'avg_interrupts' for schema compatibility)
  - average MMIO reads per input
  - average basic blocks executed per input

Run directory layout:
  {basedir}/{experiment}-aidfuzzer/{TargetGroup}/{target_leaf}_{NN}/iofuzz_cov.log

Log format (two lines per input):
    BBs executed: N  IRQs injected: M  MMIO reads: P
  [HH:MM:SS] total exec E bbl:B paths:... outofseed:O ...

Output JSON schema (same as hoedur collect_corpus_eval.py):
  {outdir}/aidfuzzer/{TargetGroup}-{target_leaf}.json
  {
    "target_flat": "<TargetGroup>-<target_leaf>",
    "fuzzer": "aidfuzzer",
    "runs": {
      "1": {
        "num_inputs":       <int>,
        "avg_interrupts":   <float|null>,
        "avg_mmio_reads":   <float|null>,
        "avg_basic_blocks": <float|null>
      },
      ...
    }
  }
"""

import argparse
import json
import re
import sys
from pathlib import Path

FUZZER_NAME = 'aidfuzzer'

RE_BB_LINE   = re.compile(
    r'BBs executed:\s*(\d+)\s+IRQs injected:\s*(\d+)\s+MMIO reads:\s*(\d+)'
)

RE_CRASH_LINE = re.compile(r"\[crash\]")

# e.g.  02-coverage-est-data-set-aidfuzzer
RE_CORPUS_DIR = re.compile(r'.+-aidfuzzer$')

# e.g.  annepro-shine_03  ->  leaf = 'annepro-shine', run_id = 3
RE_RUN_DIR = re.compile(r'^(.+)_(\d+)$')


def parse_iofuzz_cov_log(log_path: Path) -> dict:
    """Parse a single iofuzz_cov.log and return aggregate per-run statistics."""
    bbs_list  : list = []
    irqs_list : list = []
    mmio_list : list = []

    with open(log_path, 'r', errors='replace') as f:
        for line in f:
            if RE_CRASH_LINE.search(line):
                continue
            m = RE_BB_LINE.search(line)
            if m:
                bbs_list.append(int(m.group(1)))
                irqs_list.append(int(m.group(2)))
                mmio_list.append(int(m.group(3)))

    n = len(bbs_list)
    return {
        'num_inputs':       n,
        'avg_interrupts':   sum(irqs_list) / n if n else None,
        'avg_mmio_reads':   sum(mmio_list) / n if n else None,
        'avg_basic_blocks': sum(bbs_list)  / n if n else None,
    }


def collect(basedir: Path, outdir: Path) -> None:
    """Discover all iofuzz_cov.log files and write per-(target, fuzzer) JSON stats."""
    # results[target_flat] = {run_id: stats_dict}
    results: dict = {}

    for log_path in sorted(basedir.glob('*-aidfuzzer/*/*/iofuzz_cov.log')):
        # path:  {basedir}/{exp}-aidfuzzer/{TargetGroup}/{target_leaf}_{NN}/iofuzz_cov.log
        run_dir     = log_path.parent
        group_dir   = run_dir.parent  # TargetGroup dir
        target_group = group_dir.name

        m_run = RE_RUN_DIR.fullmatch(run_dir.name)
        if not m_run:
            print(f'WARNING: unexpected run dir name, skipping: {run_dir}',
                  file=sys.stderr)
            continue
        target_leaf = m_run.group(1)
        run_id      = int(m_run.group(2))
        target_flat = f'{target_group}-{target_leaf}'

        if target_flat not in results:
            results[target_flat] = {}

        print(f'  Parsing {log_path} ...', flush=True)
        results[target_flat][run_id] = parse_iofuzz_cov_log(log_path)

    fuzzer_out = outdir / FUZZER_NAME
    fuzzer_out.mkdir(parents=True, exist_ok=True)

    for target_flat, runs_data in sorted(results.items()):
        out_file = fuzzer_out / f'{target_flat}.json'
        payload = {
            'target_flat': target_flat,
            'fuzzer':      FUZZER_NAME,
            'runs': {str(k): v for k, v in sorted(runs_data.items())},
        }
        with open(out_file, 'w') as f:
            json.dump(payload, f, indent=2)
        print(f'Wrote: {out_file}', flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(
        description='Parse AIDFuzzer iofuzz_cov.log files and extract corpus eval statistics.')
    parser.add_argument('--basedir', required=True,
                        help='Base directory containing aidfuzzer fuzzing runs '
                             '(e.g., results/fuzzing-runs/aidfuzzer)')
    parser.add_argument('--outdir', required=True,
                        help='Output directory for corpus eval statistics '
                             '(e.g., results/statistics/corpus_eval/)')
    args = parser.parse_args()

    basedir = Path(args.basedir)
    outdir  = Path(args.outdir)

    if not basedir.is_dir():
        print(f'ERROR: basedir does not exist: {basedir}', file=sys.stderr)
        sys.exit(1)

    collect(basedir, outdir)


if __name__ == '__main__':
    main()
