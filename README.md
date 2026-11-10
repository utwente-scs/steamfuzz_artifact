# SteamFuzz: Boosting Firmware Fuzzing via Automatic Message Window Inference

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.22828482.svg)](https://doi.org/10.5281/zenodo.22828482)

<p align="left">
  <a href="https://www.acm.org/publications/policies/artifact-review-and-badging-current">
    <img src="https://sigsim.acm.org/conf/pads/2024/images/badges/artifacts_available_v1_1.png" height="80" alt="Artifacts Available">
  </a>
  <a href="https://www.acm.org/publications/policies/artifact-review-and-badging-current">
    <img src="https://sigsim.acm.org/conf/pads/2024/images/badges/artifacts_evaluated_functional_v1_1.png" height="80" alt="Artifacts Evaluated Functional">
  </a>
  <a href="https://www.acm.org/publications/policies/artifact-review-and-badging-current">
    <img src="https://sigsim.acm.org/conf/pads/2024/images/badges/results_reproduced_v1_1.png" height="80" alt="Results Reproduced">
  </a>
</p>

This repository contains the code of SteamFuzz, our message-aware embedded firmware fuzzer published at [ACM CCS 2026](https://www.sigsac.org/ccs/CCS2026/)

We also include the [extended version of our paper](steamfuzz_extended.pdf), which includes additional appendices.

## Contents
- `experiments/` - All scripts, data, and configurations to reproduce the SteamFuzz experiments.
- `steamfuzz/` - SteamFuzz itself.

## Reproducing our experiments

[Here](experiments/README.md) you can find the instructions to reproduce the experiments found in our paper, as well as instructions on how to install and run the fuzzer.

To recreate the paper plots using the data from our evaluation of our evaluation run, download the data from [Zenodo](https://doi.org/10.5281/zenodo.2282848)

## Cite our work

```bibtex
@inproceedings{vannielen_steamfuzz_26,
 author = {van Nielen, Jorik and Peter, Andreas and Continella, Andrea},
 booktitle = {Proceedings of the ACM Conference on Computer and Communications Security (CCS)},
 title = {SteamFuzz: Boosting Firmware Fuzzing via Automatic Message Window Inference},
 year = {2026}
}

```