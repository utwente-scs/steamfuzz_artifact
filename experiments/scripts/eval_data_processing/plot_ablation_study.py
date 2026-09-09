from functools import reduce
from pathlib import Path
from IPython import embed
from plotting import PlotStyle, PlotBackend, AltairPlotBackend, utc_timestamp_to_dt, PlotDataQuery, Selection, ChartStyle, ChartLayout
import pandas as pd
import altair as alt
import sys

from config import *


ABLATION_FUZZERS = {'hoedur_steamfuzz', 'hoedur_steamfuzz_ablation1', 'hoedur_steamfuzz_ablation2'}


def plot_ablation_study(paper):
    EXPERIMENT = active_experiment()
    plot_data = PlotDataQuery(EXPERIMENT)
    selection = plot_data.experiment_selection()

    def filter_fn(selection: Selection) -> bool:
        fuzzer = selection.attrs()['Fuzzers']
        target = selection.attrs()['TARGET']
        ablation_in_paper = EXPERIMENT['ablation_in_paper']

        if paper:
            return fuzzer in ABLATION_FUZZERS and target in ablation_in_paper
        else:  # appendix
            return fuzzer in ABLATION_FUZZERS and target not in ablation_in_paper

    selection = selection.filter(filter_fn)
    # print(selection.selections())

    pd.set_option('display.max_rows', 500)
    plot_builder = AltairPlotBackend(selection.selections())
    plot_builder = plot_builder.colorization_key('Fuzzers')
    plot_builder = plot_builder.chart_grouping_key('TARGET')

    def style_cb(group: str) -> PlotStyle:
        x_tick_values = [h * 3600 * 1000 for h in range(0, 25, 4)]
        x_label_expr = "datum.label == '00:00' && datum.value > 0 ? '24:00' : datum.label"
        return PlotStyle(
            title=group.replace('_', '-'),
            dots_every_n_minutes=60,
            y_lable='Covered Basic Blocks',
            x_lable='Time (hh:mm)',
            x_axis_tick_values=x_tick_values,
            x_axis_label_expr=x_label_expr,
        )

    plot_builder.set_selection_style_cb(style_cb)

    from plotting import color_selection_cb
    plot_builder.set_custom_color_cb(color_selection_cb)

    from plotting import name_resolver
    plot_builder.set_key_resolver_cb(name_resolver)

    charts = plot_builder.plot()
    chart = ChartLayout.cols_rows(charts, 5, 4)
    chart = ChartStyle.default_style(chart, legend_label_font_size=22)

    args = sys.argv
    if len(args) != 2:
        print(f'Usage: {args[0]} <output-path>.(svg|png)')
        exit(1)

    out_path = args[1]
    chart.save(out_path)


# paper targets
plot_ablation_study(True)
