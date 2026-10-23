#!/usr/bin/env python3
"""Scan release files, including nested tarballs, without displaying secret bytes."""
import io, pathlib, re, sys, tarfile
patterns=[rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',rb'\bsk-(?:proj-)?[A-Za-z0-9_-]{30,}',rb'\beyJ[A-Za-z0-9_-]{25,}\.[A-Za-z0-9_-]{25,}\.[A-Za-z0-9_-]{15,}']
count=0
budget=512*1024*1024
def scan(name,data,depth=0):
 global count,budget
 count+=1;budget-=len(data)
 if budget<0 or depth>5: raise ValueError('Scan resource limit: '+name)
 if pathlib.PurePosixPath(name).name in ('auth.json','state.json','.env') or any(re.search(p,data) for p in patterns): raise ValueError('Potential secret path: '+name)
 if name.endswith(('.tar.gz','.tgz')):
  with tarfile.open(fileobj=io.BytesIO(data),mode='r:gz') as archive:
   for member in archive:
    if member.isfile():
     if member.size>100*1024*1024: raise ValueError('Oversized archive member: '+name)
     scan(name+'!/'+member.name,archive.extractfile(member).read(),depth+1)
for arg in sys.argv[1:]:
 path=pathlib.Path(arg)
 for file in sorted(path.rglob('*')) if path.is_dir() else [path]:
  if file.is_file(): scan(str(file),file.read_bytes())
print(f'{count} release files/archive members scanned; no secret patterns found')
