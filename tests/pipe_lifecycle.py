#!/usr/bin/env python3
"""Run narrow streaming/maintenance tests in a disposable container; mocked systemd."""
import argparse,pathlib,subprocess
p=argparse.ArgumentParser();p.add_argument('assets');p.add_argument('--image',default='kairo-release-lab:bookworm');a=p.parse_args();repo=pathlib.Path(__file__).resolve().parents[1]
assert (repo/'SOURCE_ORIGIN.json').is_file(),'Use the public-only checkout'
subprocess.run(['docker','run','--rm','--network','none','--memory','768m','--cpus','1','--security-opt','no-new-privileges','-v',f'{repo}:/source:ro','-v',f'{pathlib.Path(a.assets).resolve()}:/assets:ro',a.image,'python3','/source/tests/pipe_lifecycle_guest.py'],check=True,timeout=180)
