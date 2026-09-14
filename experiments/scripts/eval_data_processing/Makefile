SCRIPTS := plotting/query.py \
	plotting/plotting.py \
	plotting/color.py \
	docker-image

# The active run-time profile selects the experiment to plot (see config.py).
# Charts are written per profile so switching profiles does not overwrite the
# charts of the previously plotted experiment.
BASEDIR ?= $(CURDIR)/../..
PROFILE := $(strip $(shell cat $(BASEDIR)/experiment-config/active_profile.txt))
CHARTS := charts/$(PROFILE)

ifeq ($(PROFILE),)
$(error could not read active profile from $(BASEDIR)/experiment-config/active_profile.txt)
endif

SVG_TARGETS = \
  	$(CHARTS)/sota_comparison_charts_compressed.svg \
  	$(CHARTS)/sota_comparison_charts_per_run_compressed.svg \
  	$(CHARTS)/ablation_study_charts_compressed.svg \
  	$(CHARTS)/ablation_study_charts_remaining_compressed.svg \
  	$(CHARTS)/survivability_charts_compressed.svg
# 	$(CHARTS)/cve_cov_charts_compressed.svg \
# 	$(CHARTS)/single_vs_multi_compressed.svg

PDF_TARGETS = \
	$(CHARTS)/sota_comparison_plot.pdf \
	$(CHARTS)/sota_comparison_per_run_plot.pdf \
	$(CHARTS)/ablation_study_plot.pdf \
	$(CHARTS)/ablation_study_remaining_plot.pdf \
	$(CHARTS)/survivability_plot.pdf

TARGETS = $(SVG_TARGETS) $(PDF_TARGETS)
# 	/home/user/hoedur-experiments/01-bug-finding-ability/results/figure_5_cve_coverage_plot.pdf \
# 	$(CHARTS)/ablation_study_charts.png
# 	/home/user/hoedur-experiments/02-coverage-est-data-set/results/figure_8_appendix_baseline_coverage_plot.pdf \
# 	/home/user/hoedur-experiments/03-advanced-mutations/results/figure_7_baseline_dict_coverage_plot.pdf \
# 	$(CHARTS)/cve_cov_charts.png \
# 	$(CHARTS)/single_vs_multi.png \
# 	$(CHARTS)/ablation_study_charts_remaining.png

.DEFAULT_GOAL := all
# --keep-going: if one chart fails (e.g. due to missing/corrupt data for one
# target), still build the other independent charts instead of aborting everything.
MAKEFLAGS := --jobs=$(shell nproc) --keep-going

$(CHARTS):
	mkdir -p $@

# For running the plotting environment
docker:
	@docker --version >/dev/null 2>&1 || (echo "ERROR: docker is required."; exit 1)

docker-image: docker
	bash ./env/build.sh

$(CHARTS)/cve_cov_charts.png: $(CHARTS)/cve_cov_charts.svg
	./env/run.sh inkscape --export-dpi 100 $(CHARTS)/cve_cov_charts.svg -o $(CHARTS)/cve_cov_charts.png

$(CHARTS)/cve_cov_charts.svg: $(SCRIPTS) plot_cve.py | $(CHARTS)
	./env/run.sh python3 plot_cve.py $(CHARTS)/cve_cov_charts.svg

$(CHARTS)/cve_cov_charts_compressed.svg: $(CHARTS)/cve_cov_charts.svg
	./env/run.sh svgcleaner $(CHARTS)/cve_cov_charts.svg $(CHARTS)/cve_cov_charts_compressed.svg

/home/user/hoedur-experiments/01-bug-finding-ability/results/figure_5_cve_coverage_plot.pdf: $(CHARTS)/cve_cov_charts_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=/home/user/hoedur-experiments/01-bug-finding-ability/results/figure_5_cve_coverage_plot.pdf \
		$(CHARTS)/cve_cov_charts_compressed.svg



$(CHARTS)/sota_comparison_charts.png: $(CHARTS)/sota_comparison_charts.svg
	./env/run.sh inkscape --export-dpi 100 $(CHARTS)/sota_comparison_charts.svg -o $(CHARTS)/sota_comparison_charts.png

# Produces both the aggregated chart (median + interval) and the per-run
# run-to-run comparison chart in a single invocation (grouped target).
$(CHARTS)/sota_comparison_charts.svg $(CHARTS)/sota_comparison_charts_per_run.svg &: $(SCRIPTS) plot_sota_comparison.py | $(CHARTS)
	./env/run.sh python3 plot_sota_comparison.py $(CHARTS)/sota_comparison_charts.svg

$(CHARTS)/sota_comparison_charts_compressed.svg: $(CHARTS)/sota_comparison_charts.svg
	./env/run.sh svgcleaner $(CHARTS)/sota_comparison_charts.svg $(CHARTS)/sota_comparison_charts_compressed.svg

$(CHARTS)/sota_comparison_plot.pdf: $(CHARTS)/sota_comparison_charts_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=$(CHARTS)/sota_comparison_plot.pdf \
		$(CHARTS)/sota_comparison_charts_compressed.svg

$(CHARTS)/sota_comparison_charts_per_run_compressed.svg: $(CHARTS)/sota_comparison_charts_per_run.svg
	./env/run.sh svgcleaner $(CHARTS)/sota_comparison_charts_per_run.svg $(CHARTS)/sota_comparison_charts_per_run_compressed.svg

$(CHARTS)/sota_comparison_per_run_plot.pdf: $(CHARTS)/sota_comparison_charts_per_run_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=$(CHARTS)/sota_comparison_per_run_plot.pdf \
		$(CHARTS)/sota_comparison_charts_per_run_compressed.svg



$(CHARTS)/ablation_study_charts.png: $(CHARTS)/ablation_study_charts.svg
	./env/run.sh inkscape --export-dpi 100 $(CHARTS)/ablation_study_charts.svg -o $(CHARTS)/ablation_study_charts.png

$(CHARTS)/ablation_study_charts.svg: $(SCRIPTS) plot_ablation_study.py | $(CHARTS)
	./env/run.sh python3 plot_ablation_study.py $(CHARTS)/ablation_study_charts.svg

$(CHARTS)/ablation_study_charts_compressed.svg: $(CHARTS)/ablation_study_charts.svg
	./env/run.sh svgcleaner $(CHARTS)/ablation_study_charts.svg $(CHARTS)/ablation_study_charts_compressed.svg

$(CHARTS)/ablation_study_plot.pdf: $(CHARTS)/ablation_study_charts_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=$(CHARTS)/ablation_study_plot.pdf \
		--actions 'select-all:groups\;SelectionUnGroup\;' \
		$(CHARTS)/ablation_study_charts_compressed.svg

# Keep the hoedur-experiments copy in sync if the directory exists
/home/user/hoedur-experiments/02-coverage-est-data-set/results/figure_6_baseline_coverage_plot.pdf: $(CHARTS)/ablation_study_charts_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=/home/user/hoedur-experiments/02-coverage-est-data-set/results/figure_6_baseline_coverage_plot.pdf \
		--actions 'select-all:groups\;SelectionUnGroup\;' \
		$(CHARTS)/ablation_study_charts_compressed.svg




$(CHARTS)/ablation_study_charts_remaining.png: $(CHARTS)/ablation_study_charts_remaining.svg
	./env/run.sh inkscape --export-dpi 100 $(CHARTS)/ablation_study_charts_remaining.svg -o $(CHARTS)/ablation_study_charts_remaining.png

$(CHARTS)/ablation_study_charts_remaining.svg: $(SCRIPTS) plot_ablation_study_remaining.py | $(CHARTS)
	./env/run.sh python3 plot_ablation_study_remaining.py $(CHARTS)/ablation_study_charts_remaining.svg

$(CHARTS)/ablation_study_charts_remaining_compressed.svg: $(CHARTS)/ablation_study_charts_remaining.svg
	./env/run.sh svgcleaner $(CHARTS)/ablation_study_charts_remaining.svg $(CHARTS)/ablation_study_charts_remaining_compressed.svg

$(CHARTS)/ablation_study_remaining_plot.pdf: $(CHARTS)/ablation_study_charts_remaining_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=$(CHARTS)/ablation_study_remaining_plot.pdf \
		--actions 'select-all:groups\;SelectionUnGroup\;' \
		$(CHARTS)/ablation_study_charts_remaining_compressed.svg

# Keep the hoedur-experiments copy in sync if the directory exists
/home/user/hoedur-experiments/02-coverage-est-data-set/results/figure_8_appendix_baseline_coverage_plot.pdf: $(CHARTS)/ablation_study_charts_remaining_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=/home/user/hoedur-experiments/02-coverage-est-data-set/results/figure_8_appendix_baseline_coverage_plot.pdf \
		--actions 'select-all:groups\;SelectionUnGroup\;' \
		$(CHARTS)/ablation_study_charts_remaining_compressed.svg




$(CHARTS)/single_vs_multi.png: $(CHARTS)/single_vs_multi.svg
	./env/run.sh inkscape --export-dpi 100 $(CHARTS)/single_vs_multi.svg -o $(CHARTS)/single_vs_multi.png

$(CHARTS)/single_vs_multi.svg: $(SCRIPTS) plot_single_vs_multi.py | $(CHARTS)
	./env/run.sh python3 plot_single_vs_multi.py $(CHARTS)/single_vs_multi.svg

$(CHARTS)/single_vs_multi_compressed.svg: $(CHARTS)/single_vs_multi.svg
	./env/run.sh svgcleaner $(CHARTS)/single_vs_multi.svg $(CHARTS)/single_vs_multi_compressed.svg

/home/user/hoedur-experiments/03-advanced-mutations/results/figure_7_baseline_dict_coverage_plot.pdf: $(CHARTS)/single_vs_multi_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=/home/user/hoedur-experiments/03-advanced-mutations/results/figure_7_baseline_dict_coverage_plot.pdf \
		$(CHARTS)/single_vs_multi_compressed.svg

# Build SVG + PDF + PNG
all: docker-image $(TARGETS)

# Build only SVG
svg: docker-image $(SVG_TARGETS)

$(CHARTS)/survivability_charts.svg: $(SCRIPTS) plot_survivability.py frb_timings.py | $(CHARTS)
	./env/run.sh python3 plot_survivability.py $(CHARTS)/survivability_charts.svg \
		--json $(CHARTS)/survivability_timings.json

$(CHARTS)/survivability_charts_compressed.svg: $(CHARTS)/survivability_charts.svg
	./env/run.sh svgcleaner $(CHARTS)/survivability_charts.svg $(CHARTS)/survivability_charts_compressed.svg

$(CHARTS)/survivability_plot.pdf: $(CHARTS)/survivability_charts_compressed.svg
	./env/run.sh inkscape \
		--export-type=pdf \
		--export-filename=$(CHARTS)/survivability_plot.pdf \
		$(CHARTS)/survivability_charts_compressed.svg

# Remove the charts of the active profile only
clean:
	rm -f $(CHARTS)/*.png $(CHARTS)/*.svg $(CHARTS)/*.pdf $(CHARTS)/survivability_timings.json

# Remove the charts of every profile
clean-all:
	rm -rf charts/*/
