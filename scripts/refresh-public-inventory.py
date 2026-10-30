#!/usr/bin/env python3
"""Refresh current tracked-file digests, retaining original snapshot provenance."""
import hashlib,json,pathlib,subprocess
repo=pathlib.Path(__file__).resolve().parents[1]
p=repo/'PUBLIC_FILE_MANIFEST.json';manifest=json.loads(p.read_text());old={r['path']:r for r in manifest['files']}
files=sorted(n for n in subprocess.check_output(['git','ls-files','-z'],cwd=repo,text=True).split('\0') if n and n not in ('PUBLIC_FILE_MANIFEST.json','SHA256SUMS.public'))
rows=[]
for name in files:
 path=repo/name
 assert path.is_file() and not path.is_symlink(),name
 digest=hashlib.sha256(path.read_bytes()).hexdigest()
 row=old.get(name,{'path':name,'category':'first-party','license_expression':'MIT','license_basis':'Project-authored development under the existing MIT license','source_commit':None,'source_path':name,'source_sha256':None,'transformation':'New development after the recorded source snapshot'})
 if row.get('sha256') and row['sha256']!=digest:row['transformation']='Maintained after snapshot; original source identifiers retained for provenance'
 row['sha256']=digest;rows.append(row)
manifest['files']=rows
p.write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
(repo/'SHA256SUMS.public').write_text(''.join(f"{r['sha256']}  {r['path']}\n" for r in rows))
print(f'{len(rows)} tracked-file hashes refreshed; SOURCE_ORIGIN.json unchanged')
