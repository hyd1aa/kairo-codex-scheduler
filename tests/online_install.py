#!/usr/bin/env python3
"""Disposable container HTTPS/install regressions, mocked systemd; never a VM pass."""
import argparse,pathlib,subprocess,sys
p=argparse.ArgumentParser();p.add_argument('assets');p.add_argument('--image',default='kairo-online-test');p.add_argument('--codex-fixture');a=p.parse_args();repo=pathlib.Path(__file__).resolve().parents[1]
assert (repo/'SOURCE_ORIGIN.json').is_file(), 'Mount independent public checkout only'
cmd=['docker','run','--rm','--network','none','--memory','768m','--cpus','1','--security-opt','no-new-privileges','-v',f'{repo}:/source:ro','-v',f'{pathlib.Path(a.assets).resolve()}:/assets:ro']
if a.codex_fixture:cmd+=['-v',f'{pathlib.Path(a.codex_fixture).resolve()}:/official-codex.tar.gz:ro']
cmd += [a.image,'python3','/source/tests/online_install_guest.py']
subprocess.run(cmd,check=True,timeout=180)
