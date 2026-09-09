#!/usr/bin/env python3

import os
import glob
import signal
import threading
import argparse
import subprocess
import yaml
import time

from pathlib import Path

IOFUZZ = "./iofuzz"
TARGETS_BASE = Path("/home/user/experiments/targets/arm")
SIMULATOR = "./simulator"
EXPERIMENTS_BASE = Path("/home/user/experiments")
FUZZER_NAME = "aidfuzzer"

VIRTUALENVWRAPPER_SH = "/usr/share/virtualenvwrapper/virtualenvwrapper.sh"
VIRTUALENV_ACTIVATE = "/home/user/.virtualenvs/fuzzware/bin/activate"


def _activate_env_prefix() -> str:
    """Return a shell snippet that activates the fuzzware virtualenv."""
    if os.path.isfile(VIRTUALENVWRAPPER_SH):
        return f'source "{VIRTUALENVWRAPPER_SH}" && workon fuzzware'
    # Fallback: activate the virtualenv directly
    return f'source "{VIRTUALENV_ACTIVATE}"'


def eprint(*args, **kwargs):
    import sys
    print(*args, file=sys.stderr, **kwargs)



def do_aidfuzzer_run(name, run_id, target, duration):
    target_dir = TARGETS_BASE / target
    config_path = target_dir / "config_aidfuzzer.yml"
    if not config_path.exists():
        eprint(f"[ERROR] config_aidfuzzer.yml not found for target {target}: {config_path}")
        return

    # Corpus directory: <experiments>/<name>/results/fuzzing-runs/aidfuzzer/<name>-aidfuzzer/<target>_<run_id:02d>/
    corpus_dir = EXPERIMENTS_BASE / name / "results" / "fuzzing-runs" / FUZZER_NAME / f"{name}-{FUZZER_NAME}" / f"{target}_{run_id:02d}"
    corpus_dir.mkdir(parents=True, exist_ok=True)

    iofuzz_args = " ".join([
        IOFUZZ, "fuzz",
        str(config_path),
        SIMULATOR,
        "-corpus", str(corpus_dir),
        "-core", "1",
    ])
    iofuzz_cmd = iofuzz_args  # for logging
    cmd = ["bash", "-c", f'{_activate_env_prefix()} && {iofuzz_args}']

    eprint(f"[*] Starting AIDFuzzer run: {iofuzz_cmd} timeout={duration}")

    timeout_seconds = _parse_seconds(duration)
    minutes_check = timeout_seconds//60
    log_path = corpus_dir / "iofuzz.log"
    completed = False
    for attempt in range(1, 2):  # up to 2 attempts
        eprint(f"[*] Starting attempt {attempt}/2 ({duration} = {timeout_seconds}s): {iofuzz_cmd}")
        open_mode = "w" if attempt == 1 else "a"
        with open(log_path, open_mode) as log:
            if attempt > 1:
                log.write(f"\n--- retry attempt {attempt} ---\n")
            proc = subprocess.Popen(cmd, stdout=log, stderr=log, cwd="/home/user/xxfuzzer/framework/bin")
        # Wait for the duration, but detect early exit
        for i in range(minutes_check):
            time.sleep(60)
            if proc.poll() is not None:
                eprint(f"[WARNING] aidfuzzer exited prematurely for {name}/{target} run {run_id} "
                       f"attempt {attempt}/2 (exit {proc.returncode})")
                if i > 30:
                    # not running again
                    completed = True
                break
        else:
            # Completed full duration without early exit
            os.kill(proc.pid, signal.SIGINT)
            os.waitpid(proc.pid, 0)
            completed = True
        if completed:
            break
    if not completed:
        eprint(f"[ERROR] aidfuzzer failed after all retry attempts for {name}/{target} run {run_id}, skipping coverage collection.")
        return
    eprint(f"[+] Done aidfuzzer: {name}/{target} run {run_id}")

    _collect_coverage(target_dir, corpus_dir)


def _collect_coverage(target_dir, corpus_dir):
    """Run iofuzz in replay mode to collect cov.log and plot.log for this run."""
    config_path = target_dir / "config_aidfuzzer.yml"
    cov_log = corpus_dir / "cov.log"
    plot_log = corpus_dir / "plot.log"

    cmd_parts = [
        IOFUZZ, "run",
        str(config_path),
        SIMULATOR,
        "-corpus", str(corpus_dir),
        "-cov_log", str(cov_log),
        "-plot", str(plot_log),
    ]
    filter_file = target_dir / "valid_basic_blocks.txt"
    if filter_file.exists():
        cmd_parts += ["-filter", str(filter_file)]

    iofuzz_args = " ".join(str(p) for p in cmd_parts)
    eprint(f"[*] Collecting coverage: {iofuzz_args}")
    bash_cmd = ["bash", "-c", f'{_activate_env_prefix()} && {iofuzz_args}']
    cov_log_path = corpus_dir / "iofuzz_cov.log"
    with open(cov_log_path, "w") as log:
        result = subprocess.run(bash_cmd,
                                stdout=log, stderr=log,
                                cwd="/home/user/xxfuzzer/framework/bin")
    if result.returncode != 0:
        eprint(f"[WARNING] Coverage collection exited with {result.returncode}, see {cov_log_path}")
    else:
        eprint(f"[+] Coverage collected: {cov_log}, {plot_log}")


def _parse_seconds(duration_str):
    """Convert a duration string like '24h', '30m', '3600s' or '3600' to seconds."""
    duration_str = str(duration_str).strip()
    if duration_str.endswith('h'):
        return int(duration_str[:-1]) * 3600
    elif duration_str.endswith('m'):
        return int(duration_str[:-1]) * 60
    elif duration_str.endswith('s'):
        return int(duration_str[:-1])
    else:
        return int(duration_str)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('host_run_config', type=Path)
    parser.add_argument("--cores", default=None, type=int,
                        help="Force the use of a specific number of maximum cores.")
    args = parser.parse_args()

    host_run_config = yaml.safe_load(open(args.host_run_config).read())

    # collect run list for aidfuzzer
    run_list = []
    for run in host_run_config['runs']:
        if run['fuzzer'] != FUZZER_NAME:
            continue

        name = run['output']
        target = run['target']
        run_id = run['run_id']
        duration = run['duration']

        run_list.append([name, run_id, target, duration])

    if not run_list:
        eprint("[*] No aidfuzzer runs configured, nothing to do.")
        return

    max_runs = len(run_list)

    # verify cores
    cores = args.cores
    if cores is None:
        cores = host_run_config['cores'].get(FUZZER_NAME, 0)
    if cores == 0:
        eprint('ERROR: no cores available for run list:\n', run_list)
        exit(1)

    # run in /tmp to avoid polluting the experiments directory
    os.chdir('/tmp')

    # start thread per core
    threads = []
    for core in range(cores):
        t = threading.Thread(target=runner, args=(core, run_list, max_runs))
        t.start()
        threads.append(t)

    for t in threads:
        t.join()


def runner(core, run_list, max_runs):
    eprint(core, 'start')

    while run_list:
        run_args = run_list.pop(0)
        [name, run_id, target, duration] = run_args
        eprint(core, 'run', max_runs - len(run_list), '/', max_runs, ':', run_args)

        do_aidfuzzer_run(name, run_id, target, duration)

        eprint(core, 'done', run_args)


if __name__ == '__main__':
    main()
