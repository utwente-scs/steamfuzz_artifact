#!/usr/bin/env python3
"""
Survivability plot for FirmReBugger bug-detection timings.

One plot per target sample.  The X-axis is time; the Y-axis is the number of
bug-triggers found for that sample.  A 66 % confidence interval band (based
on the spread across runs) is drawn around the median line.

Data sources
------------
aidfuzzer:
    <experiment>/results/fuzzing-runs/aidfuzzer/<exp>-aidfuzzer/<target>_<NN>/
        frb.log        -- FirmReBugger replay output (per-input sections)
        frb_plot.log   -- per-input creation timestamps (from iofuzz -plot)
        plot.log       -- fallback if frb_plot.log is not yet generated

hoedur / steamfuzz:
    <experiment>/results/fuzzing-runs/<fuzzer>/<exp>-<fuzzer>/
        TARGET-<target>-FUZZER-<fuzzer>-RUN-NN-DURATION-<d>-MODE-fuzzware.frb.log
        TARGET-<target>-FUZZER-<fuzzer>-RUN-NN-DURATION-<d>-MODE-fuzzware.corpus.tar.zst

Usage
-----
Run from scripts/eval_data_processing/ (as in the Makefile):

    python3 plot_survivability.py <output.svg> [--json <timings.json>]
                                               [--event triggered|reached]
                                               [--max-duration <seconds>]
"""

import argparse
import json
import math
import sys
from pathlib import Path

import altair as alt
import pandas as pd

from config import BASEDIR, active_experiment, FUZZER_COLOR, FUZZER_NAME as FUZZER_DISPLAY_NAME, parse_duration
from frb_timings import collect_aidfuzzer_run_timings, collect_hoedur_run_timings

# ---------------------------------------------------------------------------
# Merging timing data across all experiments/fuzzers
# ---------------------------------------------------------------------------

def collect_all_timings(experiments, event='triggered'):
    """
    Collect first-trigger times for all active experiments and fuzzers.

    Returns:
        {fuzzer_key: {experiment/target: {bug_id: [time_or_None, ...]}}}
    """
    all_data = {}

    for exp_name, experiment in experiments.items():
        exp = dict(experiment)
        exp['name'] = exp_name
        runs = exp['runs']

        for fuzzer in exp.get('fuzzer', []):
            if fuzzer not in all_data:
                all_data[fuzzer] = {}

            if fuzzer == 'aidfuzzer':
                per_target = collect_aidfuzzer_run_timings(exp, BASEDIR, event)
            elif 'hoedur' in fuzzer:
                per_target = collect_hoedur_run_timings(exp, BASEDIR, fuzzer, event)
            else:
                continue  # fuzzware etc. not supported here

            for target, bugs in per_target.items():
                key = f"{exp_name}/{target}"
                all_data[fuzzer].setdefault(key, {})
                for bug_id, times in bugs.items():
                    existing = all_data[fuzzer][key].get(bug_id, [None] * runs)
                    merged = []
                    for a, b in zip(existing, times):
                        if a is None:
                            merged.append(b)
                        elif b is None:
                            merged.append(a)
                        else:
                            merged.append(min(a, b))
                    all_data[fuzzer][key][bug_id] = merged

    return all_data


# ---------------------------------------------------------------------------
# Build long-form DataFrame for altair
# ---------------------------------------------------------------------------

def build_sample_dataframe(all_data, max_duration, interval_width=0.66):
    """
    Convert first-trigger timing dict into a long-form DataFrame with one row
    per (time-point, fuzzer, target).  Each row stores the median number of
    bugs found across runs at that time, plus the lower/upper bounds of the
    66 % confidence interval.

    Columns: time, median, lower, upper, fuzzer_name, fuzzer_color, subplot, n_bugs
    where `subplot` = target name.
    """
    rows = []
    for fuzzer_key, fuzzer_data in all_data.items():
        if fuzzer_key in ["hoedur_steamfuzz", "hoedur_steamfuzz_ablation1"]:
            continue
        display = FUZZER_DISPLAY_NAME.get(fuzzer_key, fuzzer_key)
        color = FUZZER_COLOR.get(fuzzer_key, '#888888')
        for target_key, bugs in fuzzer_data.items():
            # Exclude false-positive bugs (FP in the name) from the plot.
            bugs = {bug_id: times for bug_id, times in bugs.items()
                    if 'FP' not in bug_id}
            if not bugs:
                continue
            n_runs = len(next(iter(bugs.values())))
            n_bugs = len(bugs)
            # Strip the experiment-name prefix to get e.g. "FirmBench/contiki-router"
            target_short = target_key.split('/', 1)[1] if '/' in target_key else target_key

            if target_short.split('/')[1] in ["3Dprinter", "RIOT_CCN_LITE", "loramac", "oresat-control", "PLC", "RF-Door-lock", "Soldering_Iron", "contiki-6lowpan", "zephyr-3330", "zephyr-bt", "RF_Door_lock"]:
                continue
            
            target_short = target_short.replace('_', '-')

            # Build per-run sorted lists of trigger times.
            run_events = []
            for i in range(n_runs):
                times_for_run = []
                for times in bugs.values():
                    t = times[i]
                    if t is not None:
                        times_for_run.append(t)
                run_events.append(sorted(times_for_run))

            # All unique time points needed to represent all step functions.
            all_times = sorted(set(
                [0, max_duration] + [t for events in run_events for t in events]
            ))

            # 66 % interval half-width in sorted-run-count index space.
            interval_elm_cnt = math.floor(math.floor(n_runs * interval_width) / 2)

            for t in all_times:
                run_counts = sorted(
                    sum(1 for et in run_events[i] if et <= t)
                    for i in range(n_runs)
                )
                median_idx = n_runs // 2
                rows.append(dict(
                    time=t,
                    median=run_counts[median_idx],
                    lower=run_counts[max(0, median_idx - interval_elm_cnt)],
                    upper=run_counts[min(n_runs - 1, median_idx + interval_elm_cnt)],
                    fuzzer_name=display,
                    fuzzer_color=color,
                    subplot=target_short,
                    n_bugs=n_bugs,
                ))

    if not rows:
        return pd.DataFrame(
            columns=['time', 'median', 'lower', 'upper',
                     'fuzzer_name', 'fuzzer_color', 'subplot', 'n_bugs'])
    return pd.DataFrame(rows)


# ---------------------------------------------------------------------------
# Altair chart
# ---------------------------------------------------------------------------

def build_chart(df, max_duration, event):
    if df.empty:
        print("[WARNING] No FRB timing data found. Have you run run_firmrebugger.py?")
        return None

    color_scale = alt.Scale(
        domain=list(df['fuzzer_name'].unique()),
        range=[df.loc[df['fuzzer_name'] == n, 'fuzzer_color'].iloc[0]
               for n in df['fuzzer_name'].unique()],
    )

    # Format seconds as hh:mm (zero-padded) via a Vega expression.
    hhmm_expr = (
        "(floor(datum.value / 3600) < 10 ? '0' : '') + floor(datum.value / 3600)"
        " + ':'"
        " + (floor((datum.value % 3600) / 60) < 10 ? '0' : '') + floor((datum.value % 3600) / 60)"
    )

    # Tick positions every 3 hours.
    tick_values = list(range(0, max_duration + 1, 3 * 3600))

    x_enc = alt.X('time:Q',
                  title='Time (hh:mm)',
                  scale=alt.Scale(domain=[0, max_duration], nice=False),
                  axis=alt.Axis(labelExpr=hhmm_expr, values=tick_values))

    # Median line (step-after)
    line = alt.Chart(df).mark_line(interpolate='step-after').encode(
        x=x_enc,
        y=alt.Y('median:Q',
                title='Bugs found',
                axis=alt.Axis(format='d', tickMinStep=1)),
        color=alt.Color('fuzzer_name:N', title='Fuzzer', scale=color_scale,
                        legend=alt.Legend(orient='top')),
    )

    # 66 % confidence interval band (step-after)
    band = alt.Chart(df).mark_area(interpolate='step-after', opacity=0.2).encode(
        x=alt.X('time:Q', scale=alt.Scale(domain=[0, max_duration], nice=False),
                axis=alt.Axis(labelExpr=hhmm_expr, values=tick_values)),
        y=alt.Y('lower:Q'),
        y2=alt.Y2('upper:Q'),
        color=alt.Color('fuzzer_name:N', scale=color_scale, legend=None),
    )

    # Alphabetical, case-insensitive order, matching the other plots/table.
    subplot_order = sorted(df['subplot'].unique(), key=str.lower)

    chart = (
        alt.layer(line, band)
        .properties(width=280, height=180)
        .facet(
            facet=alt.Facet('subplot:N', title=None, sort=subplot_order),
            columns=5,
        )
        .resolve_scale(x='independent', y='independent')
    )

    return (
        chart
        .configure(font='cmr10')
        .configure_axis(labelFontSize=18, titleFontSize=18)
        .configure_title(fontSize=18, fontWeight='bold')
        .configure_header(labelFontSize=18, labelFontWeight='bold')
        .configure_legend(titleFontSize=20, labelFontSize=18, labelLimit=0)
    )


# ---------------------------------------------------------------------------
# JSON export
# ---------------------------------------------------------------------------

def export_timings_json(all_data, out_path):
    """
    Write {fuzzer: {target_key: {bug_id: [seconds_or_null, ...]}}} as JSON.
    """
    serializable = {
        fuzzer: {
            target_key: {bug_id: list(times) for bug_id, times in bugs.items()}
            for target_key, bugs in fd.items()
        }
        for fuzzer, fd in all_data.items()
    }
    with open(out_path, 'w') as fh:
        json.dump(serializable, fh, indent=2)
    print(f"[+] Exported timings to {out_path}")


# ---------------------------------------------------------------------------
# CLI  (positional: output SVG path — same convention as other plot scripts)
# ---------------------------------------------------------------------------

def _parse_args():
    p = argparse.ArgumentParser(description="Generate FRB survivability plot")
    p.add_argument('out', help="Output path for the chart (e.g. charts/survivability_charts.svg)")
    p.add_argument('--json', default=None,
                   help="Optional path to export raw timing data as JSON "
                        "(e.g. charts/survivability_timings.json).")
    p.add_argument('--event', choices=['triggered', 'reached'], default='triggered',
                   help="Which FRB event to time (default: triggered).")
    p.add_argument('--max-duration', type=int, default=None,
                   help="Fuzzing duration in seconds for x-axis extent. "
                        "Auto-detected from experiment config if not given.")
    return p.parse_args()


def _infer_max_duration(experiments):
    durations = []
    for exp in experiments.values():
        dur = exp.get('duration')
        if dur:
            durations.append(parse_duration(str(dur)))
    return max(durations) if durations else 86400  # default 24 h


def main():
    args = _parse_args()

    experiment = active_experiment()
    active = {experiment['name']: dict(experiment)}
    max_duration = 24 * 60 * 60
    print(f"[*] Max duration: {max_duration} s ({max_duration / 3600:.1f} h)")

    all_data = collect_all_timings(active, event=args.event)

    if args.json:
        export_timings_json(all_data, args.json)

    df = build_sample_dataframe(all_data, max_duration)
    chart = build_chart(df, max_duration, args.event)
    if chart is not None:
        chart.save(args.out)
        print(f"[+] Saved survivability chart to {args.out}")


if __name__ == '__main__':
    main()
