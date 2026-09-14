#!/usr/bin/env python3
"""
Generate a LaTeX table summarizing corpus evaluation statistics for the
experiment selected by the active run-time profile (see config.py).

Rows:    one per firmware target
Columns: Coverage (BBs) | Inputs in Corpus | Avg BBs/Input | Avg MMIO Reads/Input
         each split into sub-columns: Hoedur | Steamfuzz | AIDFuzzer
Each cell: min/max/avg across fuzzing runs.

Coverage data is loaded from the existing coverage pipeline (plots.json).
Inputs/BBs/MMIO data are loaded from the corpus_eval JSON files produced by
collect_corpus_eval.py (and, once implemented, the AIDFuzzer equivalent).

Output: {experiment}/results/table_corpus_eval.tex
"""

import json
import sys
from pathlib import Path
from statistics import median
from typing import Dict, List, Optional, Tuple

# Allow running from any working directory
sys.path.insert(0, str(Path(__file__).parent))

from config import active_experiment, FUZZER_LATEX, parse_duration

# parsing.py requires 'zstandard'; import lazily so the table can still be
# generated (without coverage data) even when the package is absent.
try:
    from parsing import parse_raw_data, get_last_y_val
    _PARSING_AVAILABLE = True
except ImportError:
    _PARSING_AVAILABLE = False

EXPERIMENT = active_experiment()
EXPERIMENT_NAME = EXPERIMENT['name']

# Fuzzers shown as sub-columns in the table (left to right)
TABLE_FUZZERS = ['hoedur', 'aidfuzzer', 'hoedur_steamfuzz']

# Override which fuzzer's data is used for a given display fuzzer.
# The display name (label) is kept, but data is loaded from the mapped name.
DATA_FUZZER = {
    'hoedur_steamfuzz': 'hoedur_steamfuzz_ablation2',
}

# Metric keys and their display order
# (coverage is handled separately via plots.json)
METRIC_KEYS = ['coverage', 'num_inputs', 'avg_basic_blocks', 'avg_mmio_reads', 'avg_interrupts']

CORPUS_EVAL_DIR  = EXPERIMENT['path'] / 'results' / 'statistics' / 'corpus_eval'
TABLE_OUTPUT_PATH = EXPERIMENT['path'] / 'results' / 'table_corpus_eval.tex'




# ---------------------------------------------------------------------------
# Coverage helpers (median-run selection)
# ---------------------------------------------------------------------------

def get_runs_coverage(fuzzer: str, target: str) -> Dict[str, int]:
    """Return {run_id_str: bb_count} for a given (fuzzer, target) from plots.json."""
    if not _PARSING_AVAILABLE:
        print("Parsing library not available; cannot load coverage data")
        return {}
    base_path = EXPERIMENT['path'] / 'results' / 'coverage'
    plots_json = base_path / 'plots.json'
    if not plots_json.exists():
        print(f"plots.json not found at expected location: {plots_json}")
        return {}
    with open(plots_json) as f:
        data = json.load(f).get('data', {})
    if fuzzer not in data or target not in data[fuzzer]:
        print(f"No coverage data found for fuzzer '{fuzzer}' and target '{target}' in plots.json")
        return {}
    duration = parse_duration(EXPERIMENT['duration'])
    run_cov: Dict[str, int] = {}
    for run_id, relpath in data[fuzzer][target].items():
        path = base_path / Path(relpath)
        if not path.exists():
            continue
        try:
            raw = parse_raw_data([path])
            run_cov[str(run_id)] = get_last_y_val(raw[0], duration)
        except Exception:
            pass
    return run_cov


def pick_median_run(run_cov: Dict[str, int]) -> Optional[str]:
    """Return the run_id of the run with median coverage."""
    if not run_cov:
        return None
    sorted_runs = sorted(run_cov.items(), key=lambda x: x[1])
    return sorted_runs[len(sorted_runs) // 2][0]

# ---------------------------------------------------------------------------
# Data loading helpers
# ---------------------------------------------------------------------------

def load_corpus_eval(fuzzer: str, target_flat: str) -> Optional[dict]:
    """Load corpus eval JSON for a given fuzzer and flat target name."""
    path = CORPUS_EVAL_DIR / fuzzer / f'{target_flat}.json'
    if not path.exists():
        return None
    try:
        with open(path) as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        print(f"Corpus eval file corrupt, skipping: {path} ({e})")
        return None


def target_to_flat(target: str) -> str:
    """'Aidfuzzer/annepro-shine'  ->  'Aidfuzzer-annepro-shine'"""
    return target.replace('/', '-')


# ---------------------------------------------------------------------------
# Cell formatting helpers  (single value for the median run)
# ---------------------------------------------------------------------------

def _missing() -> str:
    return r'\multicolumn{1}{c}{---}'


def fmt_int(v: Optional[float]) -> str:
    if v is None:
        return _missing()
    return str(int(round(v)))


def fmt_float(v: Optional[float], precision: int = 1) -> str:
    if v is None:
        return _missing()
    return f'{v:.{precision}f}'


def bold(cell: str) -> str:
    """Wrap a non-missing LaTeX cell value in \\textbf."""
    return f'\\textbf{{{cell}}}'


def find_best_idx(values: List[Optional[float]], higher_is_better: bool) -> Optional[int]:
    """Return the index of the best value, or None if all are missing."""
    valid = [(i, v) for i, v in enumerate(values) if v is not None]
    if not valid:
        return None
    if higher_is_better:
        return max(valid, key=lambda x: x[1])[0]
    else:
        return min(valid, key=lambda x: x[1])[0]

# ---------------------------------------------------------------------------
# Table generation
# ---------------------------------------------------------------------------

def generate_table() -> str:
    n_fuzzers  = len(TABLE_FUZZERS)   # 3
    n_groups   = 5                    # Coverage | Inputs | Avg BBs | Avg MMIO | Avg IRQs
    total_cols = n_fuzzers * n_groups # 15

    col_spec = 'l' + 'c' * total_cols

    lines: List[str] = []
    lines.append(f'\\begin{{tabularx}}{{\\textwidth}}{{{col_spec}}}')
    lines.append(r'\toprule')

    # ---- Header row 1: metric group labels ----
    group_labels = [
        'Coverage (BBs)',
        'Inputs in Corpus',
        r'Avg BBs\,/\,Input (x1000)',
        r'Avg MMIO Reads\,/\,Input',
        r'Avg IRQs\,/\,Input (Msg Win)',
    ]
    h1_parts = [r'\multirow{2}{*}{Firmware}']
    for lbl in group_labels:
        h1_parts.append(f'\\multicolumn{{{n_fuzzers}}}{{c}}{{\\textbf{{{lbl}}}}}')
    lines.append(' & '.join(h1_parts) + r' \\')

    # cmidrules separating each metric group
    cmidrules = []
    for i in range(n_groups):
        start = 2 + i * n_fuzzers
        end   = start + n_fuzzers - 1
        cmidrules.append(f'\\cmidrule(lr){{{start}-{end}}}')
    lines.append(' '.join(cmidrules))

    # ---- Header row 2: fuzzer names repeated per group ----
    fuzzer_labels = [FUZZER_LATEX.get(f, f'\\texttt{{{f}}}') for f in TABLE_FUZZERS]
    h2_parts = ['']  # empty cell under "Firmware"
    for _ in range(n_groups):
        h2_parts.extend(fuzzer_labels)
    lines.append(' & '.join(h2_parts) + r' \\')
    lines.append(r'\midrule')

    # ---- Data rows (one per target) ----
    # Alphabetical, case-insensitive order, matching the plots.
    for target in sorted(EXPERIMENT['target'], key=str.lower):
        target_flat  = target_to_flat(target)
        target_label = target.split('/')[-1].replace('_', r'\_')

        row_parts = [target_label]

        for metric_key in METRIC_KEYS:
            higher_is_better = (metric_key == 'coverage')

            # Collect raw numeric values and formatted cells for every fuzzer.
            raw_values: List[Optional[float]] = []
            cells: List[str] = []

            for fuzzer in TABLE_FUZZERS:
                data_fuzzer = DATA_FUZZER.get(fuzzer, fuzzer)

                if metric_key == 'coverage':
                    run_cov = get_runs_coverage(data_fuzzer, target)
                    mid = pick_median_run(run_cov)
                    raw_val: Optional[float] = run_cov[mid] if mid else None
                    cell = fmt_int(raw_val)
                else:
                    run_cov = get_runs_coverage(data_fuzzer, target)
                    mid = pick_median_run(run_cov)
                    corpus = load_corpus_eval(data_fuzzer, target_flat)
                    raw_val = None
                    if corpus and mid and mid in corpus['runs']:
                        raw_val = corpus['runs'][mid].get(metric_key)
                    elif corpus and corpus['runs']:
                        first_run = next(iter(corpus['runs'].values()))
                        raw_val = first_run.get(metric_key)

                    if metric_key == 'num_inputs':
                        cell = fmt_int(raw_val)
                    elif metric_key == 'avg_basic_blocks':
                        display_val = raw_val / 1000.0 if raw_val is not None else None
                        cell = fmt_float(display_val)
                        # Compare on the /1000 scale so best_idx is consistent
                        raw_val = display_val
                    elif metric_key == 'avg_interrupts':
                        cell = fmt_int(raw_val)
                    else:
                        cell = fmt_int(raw_val)

                    # Last column: annotate with avg message windows in
                    # parentheses (only Steamfuzz reports this).
                    if metric_key == 'avg_interrupts':
                        windows = None
                        if corpus and mid and mid in corpus['runs']:
                            windows = corpus['runs'][mid].get('avg_message_windows')
                        elif corpus and corpus['runs']:
                            windows = next(iter(corpus['runs'].values())).get('avg_message_windows')
                        if windows is not None:
                            cell = f'{cell} ({fmt_int(windows)})'

                raw_values.append(raw_val)
                cells.append(cell)

            # Bold the best cell in this metric group.
            best_idx = find_best_idx(raw_values, higher_is_better)
            for i, cell in enumerate(cells):
                row_parts.append(bold(cell) if i == best_idx else cell)

        lines.append(' & '.join(row_parts) + r' \\')

    lines.append(r'\arrayrulecolor{black}')
    lines.append(r'\bottomrule')
    lines.append(r'\end{tabularx}')

    return '\n'.join(lines)


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    table_body = generate_table()
    full_doc   =  table_body + '\n' 

    TABLE_OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(TABLE_OUTPUT_PATH, 'w') as f:
        f.write(full_doc)

    print(f'Wrote: {TABLE_OUTPUT_PATH}')


if __name__ == '__main__':
    main()
