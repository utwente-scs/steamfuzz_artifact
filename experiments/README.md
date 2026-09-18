# SteamFuzz Experiments

Follow these instructions to reproduce the experiments described in our [paper](../steamfuzz_extended.pdf).

## Requirements

Install the following packages on your system:
```
python3
sudo
rsync
make
docker
git
```

## Instalation

Move (or copy) the fuzzer into the experiment dir 
```
mv steamfuzz/ experiments/scripts/hoedur/
```

Install `steamfuzz`, `hoedur`, and `aidfuzzer` (~30GB disk usage):

```
cd experiments
python3 install.py
```

Set up host for AFL (used by AidFuzzer)
```
sudo ./scripts/fuzzware/set_limits_and_prepare_afl.sh
``` 

Make it a git repo so you can easily remove data between runs.
```
git init . && git add . && git commit -m "init"
```

## Reproducing the experiments.

We present three three options for recreating the plots:
1. Recreate the plots using the data from our fuzzing runs (should only take a couple of minutes on any system)
2. Run the full fuzzing and evaluation pipeline (takes about 1100 CPU days).
3. Run the fuzzing and evaluation pipeline over a subset of 4 samples (Takes about 110 CPU days)

If you run multiple options above, you need to clean the data in between runs by using the following script:
```
scripts/clean_data.py
```

### Recreating the plots

In case you do not have the computational power to run full experiments, you can also recreate the plots based on our runs.

First, copy the data of our runs (assuming you are still in `experiments/`):
```
cp -r ../data/01-main .
```

Make sure the profile is correct:

```
echo "steamfuzz" > experiment-config/active_profile.txt
```

Create the plots:
```
make -C scripts/eval_data_processing/ &> /dev/null && echo "DONE"
```

Create the table:
```
scripts/run_in_docker.sh scripts/generate_corpus_eval_table.sh &> /dev/null && echo "DONE"
```

### Running the fuzzing experiments

The full experiments are 40 samples, each executed by 5 fuzzers, 5 times per fuzzer, for 24 hours each. That is a total of 1,000 CPU days. Additionally, post processing takes some additional time per run.

We have instructions to run the full experiments, or an a subset.


Write your host and available cores to `experiments/experiment-config/available_hosts.txt`.
If you have 8 cores and 16 threads, write 16.

Example:
```
echo "localhost 100" > experiment-config/available_hosts.txt
```


Write profile to `experiments/experiment-config/active_profile.txt` (`steamfuzz`, runs over all 40 samples, or `subset`, which runs 4 samples by default.).

Example:
```
echo "steamfuzz" > experiment-config/active_profile.txt
```

> If you want to choose a custom subset of samples to run you can edit the file `scripts/eval_data_processing/config.py`.
> From line 191 onwards you can see the samples present in the `02-subset` run.
> Add or remove entries however you wish (make sure you add/remove from `target`, `include_in_paper`, and `ablation_in_paper`).
> All valid target names are present in the `01-main` entry above (starting at line 71).

Run the experiments (it is recommended to run this in `tmux`, so you can log out of the server):

```
./generate_host_run_config.py
./run_experiment.py experiment-config/host-run-configs/localhost.yml
```

Run FirmRebugger for bug evaluation:

```
python3 ./run_firmrebugger.py
```

Process fuzzing data and create plots:

```
python3 ./compute_metrics.py
```

## Figures and table

After recreating the plots or running the fuzzing experiments, you can find the results in two directories:

- All plots are created in `experiments/scripts/eval_data_processing/charts/<profile>/` (i.e., `experiments/scripts/eval_data_processing/charts/<steamfuzz or subset>/`)
- Table 1 can be found in `experiments/<experiment>/results/table_corpus_eval.tex` (i.e., `experiments/<01-main or 02-subset>/results/table_corpus_eval.tex`).

If you ran the experiments on a server with ssh, you can simply retrieve the results with `scp`:

```
scp -r <server>:steamfuzz_artifact/experiments/scripts/eval_data_processing/charts/subset/ .
scp <server>:steamfuzz_artifact/experiments/02-subset/results/table_corpus_eval.tex  .
```