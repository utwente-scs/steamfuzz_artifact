#!/usr/bin/env python3

import argparse
import os
import subprocess
from urllib.error import ContentTooShortError, HTTPError
from urllib.request import urlretrieve

from pathlib import Path

DIR = Path(os.path.dirname(os.path.realpath(__file__)))



def rebuild_docker_containers():
    # Fuzzware docker container
    subprocess.check_call([DIR.joinpath("scripts", "fuzzware", "build_fuzzware_docker.sh")])

    # Hoedur docker container
    subprocess.check_call([DIR.joinpath("scripts", "hoedur", "build_docker.sh")])

    # Aidfuzzer docker container
    subprocess.check_call([DIR.joinpath("scripts", "aidfuzzer", "pull_aidfuzzer_docker.sh")])

    # Eval Data Processing docker container
    subprocess.check_call(["make", "-C", DIR.joinpath("scripts", "eval_data_processing"), "docker-image"])

    # FirmRebugger images
    subprocess.check_call([DIR.joinpath("scripts", "hoedur-frb", "build_image.sh")])
    subprocess.check_call([DIR.joinpath("scripts", "aidfuzzer-frb", "build_image.sh")])

def check_install():
    # Run install check script
    subprocess.check_call([DIR / "scripts" / "check_install.py"])

def main():
    rebuild_docker_containers()

    try:
        check_install()
    except subprocess.CalledProcessError as err:
        print(f"[ERROR] Installation ran to completion, but install check failed.\nError: {err}")

if __name__ == "__main__":
    main()
