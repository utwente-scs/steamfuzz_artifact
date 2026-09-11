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

## instalation

Move (or copy) the fuzzer into the experiment dir 
```
mv steamfuzz/ experiments/scripts/hoedur/
```

Install `steamfuzz`, `hoedur`, and `aidfuzzer` (~30GB disk usage):

```
cd experiments
python3 install.py
```

## Recreating the plots

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
/scripts/run_in_docker.sh scripts/generate_corpus_eval_table.sh &> /dev/null && echo "DONE"
```

## Running experiments
The full experiments are 40 samples, each executed by 5 fuzzers, 5 times per fuzzer, for 24 hours each. That is a total of 1,000 CPU days. Additionally, post processing takes some additional time per run (not more than 1 hour/sample/run on average).

We have instructions to run the full experiemnts, or an a subset.


Write your host and available cores to `experiments/experiment-config/available_hosts.txt`.

Example:
```
echo "localhost 100" > experiment-config/available_hosts.txt
```


Write profile to `experiments/experiment-config/active_profile.txt` (`steamfuzz`, runs over all 40 samples, or `subset`, which runs 4 samples.).
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

## Results

All plots are created in `experiments/scripts/eval_data_processing/charts/<profile>/`
and table 1 can be found in `experiments/<experiment>/results/table_corpus_eval.tex`
(e.g. `experiments/01-main/results/table_corpus_eval.tex` for the `steamfuzz` profile).
