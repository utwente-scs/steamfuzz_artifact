import os
from pathlib import Path

import yaml


def env(var, default=None):
    return os.environ.get(var) or default


def parse_duration(value):
    if 's' in value:
        return int(value.rstrip('s'))
    elif 'm' in value:
        return int(value.rstrip('m')) * 60
    elif 'h' in value:
        return int(value.rstrip('h')) * 60 * 60
    elif 'd' in value:
        return int(value.rstrip('d')) * 60 * 60 * 24
    else:
        print(f'ERROR: unknown duration format "{value}"')
        return False


DIR = Path(os.path.dirname(os.path.realpath(__file__)))
default_base_dir = str(DIR.parents[1]) if len(DIR.parents) >= 2 else "/home/user/hoedur-experiments"
BASEDIR = Path(env('BASEDIR', default_base_dir))

FUZZER_NAME = {
    'hoedur': 'Hoedur',
    'hoedur_steamfuzz': 'SteamFuzz with interrupt interval mutations',
    # 'hoedur_steamfuzz_track_states': 'Steamfuzz_track_states',
    'hoedur_steamfuzz_ablation1': 'SteamFuzz without message window mutations',
    'hoedur_steamfuzz_ablation2': 'SteamFuzz',
    'aidfuzzer': 'AidFuzzer',
}
FUZZER_COLOR = {
    'hoedur': '#f59c0c',
    'hoedur_steamfuzz': '#109426',
    # 'hoedur_steamfuzz_track_states': '#0f71bd'
    'hoedur_steamfuzz_ablation1': '#d66368',
    'hoedur_steamfuzz_ablation2': '#0f71bd',
    'aidfuzzer': '#7b2d8b',
    # 'hoedur-dict': '#d66368',
    # 'hoedur-single-stream': '#0f71bd',
    # 'hoedur-single-stream-dict': '#a307a8',
}
FUZZER_SHAPE = {
    'hoedur': 'square',
    'hoedur_steamfuzz': 'cross',
    # 'hoedur_steamfuzz_track_states': 'diamond',
    'hoedur_steamfuzz_ablation1': 'diamond',
    'hoedur_steamfuzz_ablation2': 'triangle-right',
    'aidfuzzer': 'triangle-up',
    # 'hoedur-dict': 'diamond',
    # 'hoedur-single-stream': 'triangle-right',
    # 'hoedur-single-stream-dict': 'triangle-up',
}
FUZZER_LATEX = {
    'hoedur': '\\hoedur',
    'hoedur_steamfuzz': '\\steamfuzz',
    # 'hoedur_steamfuzz_track_states': '\\steamfuzz_track_states',
    'hoedur_steamfuzz_ablation1': '\\steamfuzzablationone',
    'hoedur_steamfuzz_ablation2': '\\steamfuzzablationtwo',
    'aidfuzzer': '\\aidfuzzer',
    # 'hoedur-single-stream': '\\shoedur',
}

EXPERIMENTS = {
    # experiment name
    '01-main': {
        'path': BASEDIR / '01-main',
        'fuzzer': [
            'hoedur',
            'hoedur_steamfuzz',
            'hoedur_steamfuzz_ablation1',
            'hoedur_steamfuzz_ablation2',
            'aidfuzzer',
            # 'hoedur_steamfuzz_track_states'
        ],
        # fuzzing targets
        'target': [
            'Aidfuzzer/annepro-shine',
            'Aidfuzzer/bcn_rfd_ncp',
            'Aidfuzzer/blehci',
            'Aidfuzzer/coord_ncp',
            'Aidfuzzer/mac_no_beacon_sleep',
            'Aidfuzzer/nmea',
            'Aidfuzzer/nobcn_ffd_ncp',
            'Aidfuzzer/sam4l_qtouch',
            'Aidfuzzer/taulab',
            'FirmBench/3Dprinter',
            'FirmBench/6LoWPAN_Receiver',
            'FirmBench/CNC',
            'FirmBench/Console',
            'FirmBench/Contiki_NG_Shell',
            'FirmBench/GPSTracker',
            'FirmBench/Gateway',
            'FirmBench/PLC',
            'FirmBench/RF_Door_lock',
            'FirmBench/RIOT_CCN_LITE',
            'FirmBench/RIOT_GNRC',
            'FirmBench/Soldering_Iron',
            'FirmBench/Zepyhr_SocketCan',
            'FirmBench/betaflight',
            'FirmBench/contiki-6lowpan',
            'FirmBench/contiki-hello-4-4',
            'FirmBench/contiki-hello-4-8',
            'FirmBench/contiki-router',
            'FirmBench/contiki-snmp',
            'FirmBench/hoverboard',
            'FirmBench/loramac',
            'FirmBench/oresat-control',
            'FirmBench/riot_gnrc_networking',
            'FirmBench/utasker_MODBUS',
            'FirmBench/utasker_USB',
            'FirmBench/zephyr-3330',
            'FirmBench/zephyr-bt',
            'FirmBench/zephyr-f429zi',
            'FirmBench/zephyr-nrf',
            'FirmBench/zephyr-sam4s',
            'FirmBench/zephyr-sampro',
        ],
        # selected targets for ablation study (paper / appendix filter)
        'include_in_paper': [
            'Aidfuzzer/annepro-shine',
            'Aidfuzzer/bcn_rfd_ncp',
            'Aidfuzzer/blehci',
            'Aidfuzzer/coord_ncp',
            'Aidfuzzer/mac_no_beacon_sleep',
            'Aidfuzzer/nmea',
            'Aidfuzzer/nobcn_ffd_ncp',
            'Aidfuzzer/sam4l_qtouch',
            'Aidfuzzer/taulab',
            'FirmBench/3Dprinter',
            'FirmBench/6LoWPAN_Receiver',
            'FirmBench/CNC',
            'FirmBench/Console',
            'FirmBench/Contiki_NG_Shell',
            'FirmBench/GPSTracker',
            'FirmBench/Gateway',
            'FirmBench/PLC',
            'FirmBench/RF_Door_lock',
            'FirmBench/RIOT_CCN_LITE',
            'FirmBench/RIOT_GNRC',
            'FirmBench/Soldering_Iron',
            'FirmBench/Zepyhr_SocketCan',
            'FirmBench/betaflight',
            'FirmBench/contiki-6lowpan',
            'FirmBench/contiki-hello-4-4',
            'FirmBench/contiki-hello-4-8',
            'FirmBench/contiki-router',
            'FirmBench/contiki-snmp',
            'FirmBench/hoverboard',
            'FirmBench/loramac',
            'FirmBench/oresat-control',
            'FirmBench/riot_gnrc_networking',
            'FirmBench/utasker_MODBUS',
            'FirmBench/utasker_USB',
            'FirmBench/zephyr-3330',
            'FirmBench/zephyr-bt',
            'FirmBench/zephyr-f429zi',
            'FirmBench/zephyr-nrf',
            'FirmBench/zephyr-sam4s',
            'FirmBench/zephyr-sampro',
        ],
    'ablation_in_paper': [
        'Aidfuzzer/annepro-shine',
        'Aidfuzzer/bcn_rfd_ncp',
        'Aidfuzzer/blehci',
        'Aidfuzzer/coord_ncp',
        'Aidfuzzer/mac_no_beacon_sleep',
        'Aidfuzzer/nmea',
        'Aidfuzzer/nobcn_ffd_ncp',
        'FirmBench/3Dprinter',
        'FirmBench/6LoWPAN_Receiver',
        'FirmBench/PLC',
        'FirmBench/RIOT_GNRC',
        'FirmBench/Soldering_Iron',
        'FirmBench/contiki-6lowpan',
        'FirmBench/contiki-hello-4-4',
        'FirmBench/contiki-hello-4-8',
        'FirmBench/contiki-snmp',
        'FirmBench/riot_gnrc_networking',
        'FirmBench/utasker_MODBUS',
        'FirmBench/utasker_USB',
        'FirmBench/zephyr-nrf',
    ],
    },
    # experiment name
    '02-subset': {
        'path': BASEDIR / '02-subset',
        'fuzzer': [
            'hoedur',
            'hoedur_steamfuzz',
            'hoedur_steamfuzz_ablation1',
            'hoedur_steamfuzz_ablation2',
            'aidfuzzer',
            # 'hoedur_steamfuzz_track_states'
        ],
        # fuzzing targets
        'target': [
            'Aidfuzzer/bcn_rfd_ncp',
            'Aidfuzzer/blehci',
            'FirmBench/3Dprinter',
            'FirmBench/riot_gnrc_networking',
        ],
        # selected targets for ablation study (paper / appendix filter)
        'include_in_paper': [
            'Aidfuzzer/bcn_rfd_ncp',
            'Aidfuzzer/blehci',
            'FirmBench/3Dprinter',
            'FirmBench/riot_gnrc_networking',
        ],
    'ablation_in_paper': [
        'Aidfuzzer/bcn_rfd_ncp',
        'Aidfuzzer/blehci',
        'FirmBench/3Dprinter',
        'FirmBench/riot_gnrc_networking',
    ],
    },
}

# set name for easy usage in scripts
for name, experiment in EXPERIMENTS.items():
    experiment['name'] = name


def load_file(path):
    if not os.path.isfile(path):
        print(f'ERROR: config file "{path}" missing')

    try:
        return open(path).read()
    except Exception as e:
        print(f'ERROR: could not load config file "{path}": {e}')
        exit(1)


# load eval profile
def load_run_time_profile():
    active_profile = load_file(
        BASEDIR / 'experiment-config' / 'active_profile.txt').strip()

    try:
        profiles = yaml.safe_load(
            load_file(BASEDIR / 'experiment-config' / 'profiles.yml'))

        return profiles[active_profile]
    except Exception as e:
        print(f'ERROR: failed to parse config file "profiles.yml": {e}')
        exit(1)


# apply selected profile
PROFILE = load_run_time_profile()

for name in PROFILE:
    if name not in EXPERIMENTS:
        print(f'ERROR: active profile references unknown experiment "{name}", '
              f'known experiments: {", ".join(EXPERIMENTS)}')
        exit(1)

# the profile selects which experiments are active: drop the rest so every
# script iterating EXPERIMENTS only sees experiments the profile configured
for name in [name for name in EXPERIMENTS if name not in PROFILE]:
    del EXPERIMENTS[name]

for (name, profile) in PROFILE.items():
    EXPERIMENTS[name]['runs'] = profile['runs']
    if 'cores_per_run' in profile:
        EXPERIMENTS[name]['cores'] = profile['cores_per_run']

    # verify human readable duration format is valid
    duration = profile['duration']
    if parse_duration(duration):
        EXPERIMENTS[name]['duration'] = duration
    else:
        exit(1)


def active_experiment():
    """The experiment the single-experiment scripts (plots, tables) work on.

    The active run-time profile normally selects exactly one experiment; set
    EXPERIMENT=<name> to pick one when a profile activates several.
    """
    name = env('EXPERIMENT')

    if name is None:
        if len(EXPERIMENTS) != 1:
            print(f'ERROR: {len(EXPERIMENTS)} experiments active '
                  f'({", ".join(EXPERIMENTS)}), set EXPERIMENT=<name> to select one')
            exit(1)
        return next(iter(EXPERIMENTS.values()))

    if name not in EXPERIMENTS:
        print(f'ERROR: experiment "{name}" is not active in the current profile, '
              f'active experiments: {", ".join(EXPERIMENTS)}')
        exit(1)

    return EXPERIMENTS[name]

# bug description to CVE lookup table
BUG_DESCRIPTION = {
    # new Bugs (CVE)
    'new-Bug-ipv6_routing_infinite_recursion': 'CVE-2023-29001',
    'new-Bug-unchecked_sdu_length': 'CVE-2023-23609',
    'new-Bug-l2cap_mtu_6lo_output_packetbuf_oob_write': 'CVE-2023-28116',
    'new-Bug-invalid-init-le_read_buffer_size': 'CVE-2023-0397',
    'new-Bug-sent_cmd_shared_ref_race': 'CVE-2023-1422',
    'new-Bug-hci_prio_event_alloc_err_handling': 'CVE-2023-1423',
    'new-Bug-hci-send_sync-dangling-sema-ref': 'CVE-2023-1901',
    'new-Bug-hci-send_sync-dangling-conn-state-ref': 'CVE-2023-1902',

    # fixed Bugs
    'fixed-Bug-SRH_too_many_segments_left': 'FIXED-1',
    'fixed-Bug-invalid_SRH_address_pointer': 'FIXED-2',
    'fixed-Bug-uncompress_hdr_iphc_oob_write': 'FIXED-3',
    'fixed-Bug-6lo_firstfrag_oob_write': 'FIXED-4',
    'fixed-Bug-snmp_oid_decode_oid_oob': 'FIXED-5',
    'fixed-Bug-snmp_oid_copy-missing-terminator-oob': 'FIXED-6',
    'fixed-Bug-snmp_engine_get_bulk-varbinds_length-oob': 'FIXED-7',
    'fixed-Bug-fragment_header_len': 'FIXED-8',
    'fixed-Bug-k_poll-race-condition': 'FIXED-9',
    'fixed-Bug-bt_att-resp-timeout-null-ptr': 'FIXED-10',
    'fixed-Bug-bt-periph-update_conn_param-work-double-submit': 'FIXED-11',
    'fixed-Bug-double-bt_att_chan_req_send-null-ptr': 'FIXED-12',
}
