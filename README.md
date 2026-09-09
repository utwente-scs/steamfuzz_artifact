Firmware fuzzing with automatic message window inference.

## Contents
- `experiments/` - all scripts, data, and configurations to reproduce the SteamFuzz experiments.
- `steamfuzz/` - the steamfuzz fuzzer itself.

## Requirements

Install the following packages on your system:
```
python3
sudo
rsync
make
docker
```

## Quickstart experiments

Move the fuzzer into the experiment dir (or copy, if you want)
```
mv steamfuzz/ experiments/scripts/hoedur/
```

Install `steamfuzz`, `hoedur`, and `aidfuzzer` (~30GB disk usage):

```
cd experiments
python3 install.py
```

Write your host and available cores to `experiments/experiment-config/available_hosts.txt`.

Example:
```
echo "localhost 100" > experiment-config/available_hosts.txt
```


Write profile to `experiments/experiment-config/active_profile.txt` (`steamfuzz` or `subset`).
Example:
```
echo "steamfuzz" > experiment-config/active_profile.txt
```

Run the experiments:

```
./generate_host_run_config.py
./run_experiment.py experiment-config/host-run-configs/localhost.yml
```

Run FirmRebugger for bug evaluation:

```
python3 ./run_firmrebugger.py
```

Create plots:

```
python3 ./compute_metrics.py
```


All plots are created in `experiments/scripts/eval_data_processing/charts/<profile>/`
and table 1 can be found in `experiments/<experiment>/results/table_corpus_eval.tex`
(e.g. `experiments/01-main/results/table_corpus_eval.tex` for the `steamfuzz` profile).
