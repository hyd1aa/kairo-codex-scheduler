#!/usr/bin/env python3
"""Decode every Git object, including unreachable blobs; never print secret bytes."""
import pathlib,re,subprocess
repo=pathlib.Path(__file__).resolve().parents[1]
patterns=[rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',rb'\bsk-(?:proj-)?[A-Za-z0-9_-]{30,}',rb'\beyJ[A-Za-z0-9_-]{25,}\.[A-Za-z0-9_-]{25,}\.[A-Za-z0-9_-]{15,}']
objects=subprocess.check_output(['git','cat-file','--batch-all-objects','--batch-check=%(objectname) %(objecttype) %(objectsize)'],cwd=repo).splitlines()
count=0
for record in objects:
 oid,kind,size=record.split()
 if kind!=b'blob':continue
 assert int(size)<=100*1024**2,'Oversized blob'
 data=subprocess.check_output(['git','cat-file','blob',oid.decode()],cwd=repo)
 assert not any(re.search(p,data) for p in patterns),'Potential secret in Git object '+oid.decode()
 count+=1
print(f'{count} Git blobs (all objects, including unreachable) passed configured secret checks')
