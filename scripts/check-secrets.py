#!/usr/bin/env python3
"""Scan this source tree, including an extracted source archive without .git."""
import json,pathlib,re,subprocess,sys
repo=pathlib.Path(__file__).resolve().parents[1]
if (repo/'.git').exists():
 files=[p for p in subprocess.check_output(['git','ls-files','-z'],cwd=repo,text=True).split('\0') if p]
else:
 inventory=json.loads((repo/'PUBLIC_FILE_MANIFEST.json').read_text())
 files=[p['path'] for p in inventory['files']]+[p['path'] for p in inventory['inventory_files']]
patterns=[rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',rb'\bsk-(?:proj-)?[A-Za-z0-9_-]{30,}',rb'\beyJ[A-Za-z0-9_-]{25,}\.[A-Za-z0-9_-]{25,}\.[A-Za-z0-9_-]{15,}']
failed=[]
for name in files:
 path=repo/name
 if not path.is_file() or path.is_symlink() or pathlib.PurePosixPath(name).name in ('auth.json','state.json','.env'):
  failed.append(name);continue
 if any(re.search(pattern,path.read_bytes()) for pattern in patterns):failed.append(name)
if failed:
 print('Potential secret/unsafe paths:',*failed,sep='\n');sys.exit(1)
print(f'{len(files)} source files scanned without using a parent repository; no configured secret patterns')
