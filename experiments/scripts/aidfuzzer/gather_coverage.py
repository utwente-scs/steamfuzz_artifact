#!/usr/bin/env python3
"""
Convert per-run cov.log / plot.log files (written by aidfuzzer-run_experiment.py
after each fuzzing run) into the format expected by the evaluation pipeline.

Run directories are discovered under --basedir using the layout created by
aidfuzzer-run_experiment.py:

  <basedir>/<experiment>-aidfuzzer/<TargetGroup>/<target_leaf>_<NN>/

The target name used as the plots.json key is "<TargetGroup>/<target_leaf>"
(matching the entries in config.py EXPERIMENTS).

Outputs written to --outdir:

  charts/aidfuzzer/<TargetGroup>-<target_leaf>-run-<NN>.json
      Plain JSON: {"coverage_translation_blocks": [{"x": elapsed_sec, "y": bb_count}, ...]}
      One entry per line of plot.log.

  summary/aidfuzzer/<TargetGroup>-<target_leaf>-run-<NN>.txt
      Sorted "0x<hex>" BB addresses from cov.log, one per line.

  plots.json  (created or updated in-place)
      {"data": {"aidfuzzer": {"<TargetGroup>/<target_leaf>": {<run_no>: "<relpath>", ...}}}}

plot.log format:  <unix_timestamp> <cumulative_bb_count>
cov.log format:   EZCOV VERSION: 1 / 0x0000c120, 4, [ MAIN ] / ...
"""

import argparse
import json
import re
import pathlib

FUZZER_NAME = "aidfuzzer"
JSON_ROOT_NAME = "coverage_translation_blocks"
COVERAGE_PLOTDATA_NAME_FORMAT = "{}-run-{:02d}.json"
COVERAGE_SUMMARY_NAME_FORMAT = "{}-run-{:02d}.txt"
PLOTS_JSON_NAME = "plots.json"


def create_parser():
    parser = argparse.ArgumentParser(description="AIDFuzz coverage conversion")
    parser.add_argument('--basedir', required=True,
                        help="Base directory containing aidfuzzer fuzzing runs "
                             "(e.g., results/fuzzing-runs/aidfuzzer)")
    parser.add_argument('--outdir', required=True,
                        help="Output coverage directory (e.g., results/coverage)")
    return parser


_RUN_DIR_RE = re.compile(r'^(.+)_(\d+)$')


def _natural_sort_key(s: str) -> list:
    return [int(c) if c.isdigit() else c.lower() for c in re.split(r'(\d+)', s)]


def find_run_dirs(basedir):
    """Return sorted list of (run_dir, target_path, run_no) tuples.

    Layout: <basedir>/<corpus_root>/<TargetGroup>/<target_leaf>_<NN>/
    """
    results = []
    for corpus_root in pathlib.Path(basedir).iterdir():
        if not corpus_root.is_dir():
            continue
        for group_dir in corpus_root.iterdir():
            if not group_dir.is_dir():
                continue
            for entry in group_dir.iterdir():
                if not entry.is_dir():
                    continue
                m = _RUN_DIR_RE.match(entry.name)
                if not m:
                    continue
                target_leaf = m.group(1)
                run_no = int(m.group(2))
                target_path = str(group_dir.relative_to(corpus_root) / target_leaf)
                results.append((entry, target_path, run_no))
    return sorted(results, key=lambda t: (_natural_sort_key(t[1]), t[2]))


def parse_plot_log(plot_log):
    """Parse plot.log -> [{"x": elapsed_sec, "y": bb_count}, ...]"""
    data = []
    with open(plot_log) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith('#'):
                continue
            parts = line.split()
            if len(parts) < 2:
                continue
            try:
                ts, count = int(parts[0]), int(parts[1])
            except ValueError:
                continue
            data.append({"x": ts, "y": count})
    return data


def parse_cov_log(cov_log):
    """Parse cov.log (EZCOV format) -> sorted list of BB addresses."""
    addrs = set()
    with open(cov_log) as f:
        for line in f:
            line = line.strip()
            if not line or line.upper().startswith('EZCOV'):
                continue
            addr_str = line.split(',', 1)[0].strip()
            try:
                addrs.add(int(addr_str, 16))
            except ValueError:
                pass
    return sorted(addrs)


def main():
    args = create_parser().parse_args()
    basedir = pathlib.Path(args.basedir)
    out_dir = pathlib.Path(args.outdir)

    if not basedir.exists():
        print(f"[ERROR] Base directory '{basedir}' does not exist")
        exit(2)

    (out_dir / "charts" / FUZZER_NAME).mkdir(parents=True, exist_ok=True)
    (out_dir / "summary" / FUZZER_NAME).mkdir(parents=True, exist_ok=True)

    run_dirs = find_run_dirs(basedir)
    print(f"[+] Found {len(run_dirs)} run directories")

    plot_data_lookup_dict: dict = {}

    for run_dir, target_path, run_no in run_dirs:
        plot_log = run_dir / "plot.log"
        cov_log = run_dir / "cov.log"

        if not plot_log.exists():
            print(f"[WARNING] plot.log missing in {run_dir}, skipping")
            continue

        print(f"[*] {target_path} run {run_no:02d}  ({run_dir})")

        safe_target = target_path.replace("/", "-")
        chart_relpath = str(pathlib.Path("charts") / FUZZER_NAME /
                            COVERAGE_PLOTDATA_NAME_FORMAT.format(safe_target, run_no))
        chart_abs = out_dir / chart_relpath
        summary_abs = out_dir / "summary" / FUZZER_NAME / COVERAGE_SUMMARY_NAME_FORMAT.format(safe_target, run_no)

        with open(chart_abs, "w") as f:
            json.dump({JSON_ROOT_NAME: parse_plot_log(plot_log)}, f)

        if cov_log.exists():
            summary_abs.write_text("\n".join(f"{a:#x}" for a in parse_cov_log(cov_log)))

        plot_data_lookup_dict.setdefault(target_path, {})[run_no] = chart_relpath

    plots_json_path = out_dir / PLOTS_JSON_NAME
    plots_json = json.loads(plots_json_path.read_text()) if plots_json_path.exists() else {"data": {}}
    plots_json.setdefault("data", {})[FUZZER_NAME] = plot_data_lookup_dict
    plots_json_path.write_text(json.dumps(plots_json, indent=4))
    print(f"[+] Updated {plots_json_path}")


if __name__ == "__main__":
    main()


if __name__ == "__main__":
    main()
