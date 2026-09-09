#!/usr/bin/env python3

import argparse
import os
import subprocess
import yaml
import signal
import time
import glob

from pathlib import Path

DIR = Path(os.path.dirname(os.path.realpath(__file__)))

def create_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument('host_run_config', type=Path)
    parser.add_argument("--skip-cpufreq", default=False, action="store_true", help="Tell AFL to not be worried about the CPU scaling setting. This may be useful to not apply settings that require root privileges.")

    return parser

def check_clean_result_dirs():
    clean = True
    for pattern in [
        # '*/results/fuzzing-runs/hoedur*/*/*.corpus.tar.zst',
        # '*/results/fuzzing-runs/hoedur*/*/*.report.bin.zst',
        # '*/results/fuzzing-runs/fuzzware/**/fuzzware-project-*'
        ]:

        if len(glob.glob(str(DIR / pattern), recursive=True)) > 0:
            print(f'ERROR: found previous results "{pattern}"!')
            clean = False

    if not clean:
        print('NOTE: run "scripts/run_in_docker.sh scripts/clean_data.py" to clean old data.')

    return clean

def parse_yaml(path):
    with open(path, "r") as f:
        return yaml.safe_load(f)

def run_fuzzware_docker(cmd_args, max_cores, run_sync=False):
    docker_cmd_args = [ "docker", "run",
        "--rm", "-it",
        "--user", f"{os.getuid()}:{os.getgid()}",
        "--env", "HOME=/home/user",
        "--env", "PYTHON_EGG_CACHE=/tmp/.cache",
        "--mount", "type=bind,source="+str(DIR)+",target=/home/user/fuzzware/targets",
        "--cpus=" + f"{max_cores:d}",
        "fuzzware:fuzzware-hoedur-eval"
    ]

    if run_sync:
        return subprocess.run(docker_cmd_args + cmd_args + ["--cores", f"{max_cores:d}"])
    else:
        return subprocess.Popen(docker_cmd_args + cmd_args, preexec_fn=os.setsid)

def run_hoedur_docker(cmd_args, max_cores, run_sync=False):
    docker_cmd_args = [
        "docker",
        "run",
        "--rm",
        "--user", f"{os.getuid()}:{os.getgid()}",
        "--env", "HOME=/home/user",
        "--mount", f"src={DIR},target=/home/user/hoedur-experiments,type=bind",
        "--mount", f"src={DIR}/targets,target=/home/user/hoedur-targets,type=bind",
        "-t",
        "hoedur-fuzzware"
    ]

    if run_sync:
        return subprocess.run(docker_cmd_args + cmd_args + ["--cores", f"{max_cores:d}"])
    else:
        return subprocess.Popen(docker_cmd_args + cmd_args)

def run_aidfuzzer_docker(cmd_args, max_cores, run_sync=False):
    script = [str(DIR / "scripts" / "aidfuzzer" / "run_aidfuzzer_docker.sh")]

    if run_sync:
        return subprocess.run(script + cmd_args + ["--cores", f"{max_cores:d}"])
    else:
        return subprocess.Popen(script + cmd_args)

def sanity_check_fuzzware(cmd_args, max_cores):
    p = run_fuzzware_docker(cmd_args + ["--sanity-check"], max_cores, True)
    return p.returncode == 0

def main():
    parser = create_parser()
    args = parser.parse_args()
    host_run_config = Path(args.host_run_config)
    skip_cpu_freq = args.skip_cpufreq

    if not host_run_config.exists():
        print(f"[ERROR] Host run configuration does not exist: {host_run_config}")
        exit(1)

    cfg = parse_yaml(host_run_config)
    max_cores_fuzzware = cfg.get("cores", {}).get("fuzzware", 0)
    max_cores_hoedur = cfg.get("cores", {}).get("hoedur", 0)
    max_cores_aidfuzzer = cfg.get("cores", {}).get("aidfuzzer", 0)
    combined_cores = max_cores_fuzzware + max_cores_hoedur + max_cores_aidfuzzer

    runs_fuzzware = [entry['cores_per_run'] for entry in cfg["runs"] if entry["fuzzer"].lower() == "fuzzware"]
    min_req_fuzzware = max(runs_fuzzware) if runs_fuzzware else 0
    runs_hoedur = [entry['cores_per_run'] for entry in cfg["runs"] if "hoedur" in entry["fuzzer"].lower()]
    min_req_hoedur = max(runs_hoedur) if runs_hoedur else 0
    runs_aidfuzzer = [entry['cores_per_run'] for entry in cfg["runs"] if entry["fuzzer"].lower() == "aidfuzzer"]
    min_req_aidfuzzer = max(runs_aidfuzzer) if runs_aidfuzzer else 0

    need_sync_run = False
    if min_req_fuzzware > max_cores_fuzzware:
        need_sync_run = True
        print(f"[WARNING] Not enough cores assigned to Fuzzware (required: {min_req_fuzzware}, got: {max_cores_fuzzware}). Need to combine cores and run synchronously. This may take longer to execute overall.")
    elif min_req_hoedur > max_cores_hoedur:
        need_sync_run = True
        print(f"[WARNING] Not enough cores assigned to Hoedur (required: {min_req_hoedur}, got: {max_cores_hoedur}). Need to combine cores and run synchronously. This may take longer to execute overall.")
    elif min_req_aidfuzzer > max_cores_aidfuzzer:
        need_sync_run = True
        print(f"[WARNING] Not enough cores assigned to AIDFuzz (required: {min_req_aidfuzzer}, got: {max_cores_aidfuzzer}). Need to combine cores and run synchronously. This may take longer to execute overall.")
    else:
        print(f"[+] Enough cores available to each fuzzer to run in parallel.")

    if combined_cores < min_req_fuzzware or combined_cores < min_req_hoedur or combined_cores < min_req_aidfuzzer:
        print(f"[ERROR] Not enough cores overall to run the experiment (available: {combined_cores}, required for Hoedur: {min_req_hoedur}, required for Fuzzware: {min_req_fuzzware}, required for AIDFuzz: {min_req_aidfuzzer})")
        exit(2)

    CMD_FUZZWARE = [
        Path("scripts", "fuzzware", "run_experiment.py"),
        host_run_config.absolute().relative_to(DIR)
    ]
    if skip_cpu_freq:
        CMD_FUZZWARE.append("--skip-cpufreq")

    CMD_HOEDUR = [
        "python3",
        Path("scripts", "hoedur-run_experiment.py"),
        host_run_config.absolute().relative_to(DIR)
    ]

    CMD_AIDFUZZER = [
        "python3",
        Path("/home", "user", "experiments","scripts", "aidfuzzer", "aidfuzzer-run_experiment.py"),
        Path("/home", "user", "experiments") / host_run_config.absolute().relative_to(DIR)
    ]

    cores_used_fuzzware = combined_cores if need_sync_run else max_cores_fuzzware
    cores_used_hoedur = combined_cores if need_sync_run else max_cores_hoedur
    cores_used_aidfuzzer = combined_cores if need_sync_run else max_cores_aidfuzzer
    if max_cores_fuzzware != 0:
        if sanity_check_fuzzware(CMD_FUZZWARE, cores_used_fuzzware):
            print("[+] Sanity checks for Fuzzware succeeded.")
        else:
            print("[ERROR] Sanity checks for Fuzzware failed. Exiting...")
            exit(3)

    if not check_clean_result_dirs():
        print("[ERROR] There are still leftover files in the results directories. This may introduce unforeseen/inconsistent result states. Please clean these via the clean scripts provided in this repository.")
        exit(4)

    proc_fuzzware = proc_hoedur = proc_aidfuzzer = None

    if max_cores_fuzzware != 0:
        proc_fuzzware = run_fuzzware_docker(CMD_FUZZWARE, cores_used_fuzzware, need_sync_run)
        if need_sync_run and proc_fuzzware.returncode != 0:
            print("[WARNING] Fuzzware process did not run to successful completion. Not running Hoedur...")
            exit(5)
    elif len(runs_fuzzware) > 0:
        print(f"[WARNING] No cores are configured for Fuzzware, skipping {len(runs_fuzzware)} Fuzzware experiments")

    if max_cores_hoedur != 0:
        proc_hoedur = run_hoedur_docker(CMD_HOEDUR, cores_used_hoedur, need_sync_run)
        if need_sync_run and proc_hoedur.returncode != 0:
            print("[WARNING] Hoedur process did not run to successful completion. Stopping...")
            exit(5)
    elif len(runs_hoedur) > 0:
        print(f"[WARNING] No cores are configured for Hoedur, skipping {len(runs_hoedur)} Hoedur experiments")

    if max_cores_aidfuzzer != 0:
        proc_aidfuzzer = run_aidfuzzer_docker(CMD_AIDFUZZER, cores_used_aidfuzzer, need_sync_run)
        if need_sync_run and proc_aidfuzzer.returncode != 0:
            print("[WARNING] AIDFuzz process did not run to successful completion. Stopping...")
            exit(5)
    elif len(runs_aidfuzzer) > 0:
        print(f"[WARNING] No cores are configured for AIDFuzz, skipping {len(runs_aidfuzzer)} AIDFuzz experiments")

    if not need_sync_run:
        stop = False
        try:
            if proc_fuzzware is not None:
                proc_fuzzware.wait()
                if proc_fuzzware.returncode != 0:
                    print("[ERROR] Fuzzware did not complete successfully.")
                    stop = True
                else:
                    print("[+] Fuzzware experiment completed successfully.")

            if not stop and proc_hoedur is not None:
                print("[*] Now waiting for Hoedur")
                proc_hoedur.wait()
                if proc_hoedur.returncode != 0:
                    print("[ERROR] Hoedur did not complete successfully.")
                    exit(5)

            if not stop and proc_aidfuzzer is not None:
                print("[*] Now waiting for AIDFuzz")
                proc_aidfuzzer.wait()
                if proc_aidfuzzer.returncode != 0:
                    print("[ERROR] AIDFuzz did not complete successfully.")
                    exit(5)
                else:
                    print("[+] AIDFuzz experiment completed successfully.")
        finally:
            print("[*] Cleaning up processes")
            if proc_fuzzware is not None:
                if not proc_fuzzware.poll():
                    print("[*] Fuzzware process still alive, terminating...")
                    proc_fuzzware.kill()
            if proc_hoedur is not None:
                try:
                    gid = os.getpgid(proc_hoedur.pid)

                    while not proc_hoedur.poll():
                        print("[*] Hoedur process still alive, terminating...")
                        try:
                            os.killpg(gid, signal.SIGINT)
                        except KeyboardInterrupt:
                            pass
                        time.sleep(1)
                except:
                    pass
            if proc_aidfuzzer is not None:
                if not proc_aidfuzzer.poll():
                    print("[*] AIDFuzz process still alive, terminating...")
                    proc_aidfuzzer.kill()
            # os.killpg(0, 9)

    print("[*] Running experiment all done.")

if __name__ == '__main__':
    main()
