#!/usr/bin/env python3
"""Check exported snapshot bytes/licenses/privacy; never install or call models."""
import hashlib,json,pathlib,re,subprocess
repo=pathlib.Path(__file__).resolve().parents[1]
manifest=json.loads((repo/'PUBLIC_FILE_MANIFEST.json').read_text());origin=json.loads((repo/'SOURCE_ORIGIN.json').read_text())
assert manifest['project_license']==origin['project_license']=='MIT'
assert (repo/'LICENSE').read_text().startswith('MIT License\n')
assert 'license = "MIT"' in (repo/'Cargo.toml').read_text()
assert (repo/'NOTICE').is_file() and (repo/'THIRD_PARTY_NOTICES.md').is_file()
expected={p['path'] for p in manifest['files']}|{p['path'] for p in manifest['inventory_files']}
actual={p.relative_to(repo).as_posix() for p in repo.rglob('*') if p.is_file() and '.git' not in p.relative_to(repo).parts and not p.relative_to(repo).parts[0] in ('target','sandbox','dist')}
assert actual==expected,('Unlisted or missing files',sorted(actual^expected))
patterns=[rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',rb'\bsk-(?:proj-)?[A-Za-z0-9_-]{30,}',rb'\beyJ[A-Za-z0-9_-]{25,}\.[A-Za-z0-9_-]{25,}\.[A-Za-z0-9_-]{15,}']
for row in manifest['files']:
 p=repo/row['path'];assert p.is_file() and not p.is_symlink()
 data=p.read_bytes();assert hashlib.sha256(data).hexdigest()==row['sha256'],row['path']
 assert row['license_basis'] and row['license_expression']
 assert not any(re.search(pattern,data) for pattern in patterns),row['path']
 assert not row['path'].endswith(('.bundle','.tar.gz','.qcow2','.img','.log')),row['path']
 assert p.name not in ('auth.json','state.json','.env','id_rsa','id_ed25519'),row['path']
subprocess.run(['sha256sum','--quiet','-c','SHA256SUMS.public'],cwd=repo,check=True)
if (repo/'.git').exists():
 assert not subprocess.check_output(['git','remote'],cwd=repo).strip()
 assert not subprocess.check_output(['git','tag'],cwd=repo).strip()
 # This verifier protects the initial export only; future development must deliberately update provenance.
 assert subprocess.check_output(['git','rev-list','--all','--count'],cwd=repo,text=True).strip()=='1'
 assert not (repo/'.git/objects/info/alternates').exists()
 assert not (repo/'.git/shallow').exists()
 subprocess.run(['git','fsck','--full','--no-reflogs'],cwd=repo,check=True,stdout=subprocess.DEVNULL)
 subprocess.run(['python3','tests/git_secret_scan.py'],cwd=repo,check=True)
print(json.dumps({'public_files':len(expected),'hashes_and_license_inventory':'passed','secret_patterns':'passed; host-specific review is external','history':'single isolated root when Git present','early_python_source_included':False,'owner_licensing_authority_confirmed':False}))
