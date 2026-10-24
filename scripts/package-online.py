#!/usr/bin/env python3
"""Package only the independent public checkout; default HTTPS/SHA256 release."""
import argparse,hashlib,json,pathlib,re,subprocess,tarfile,tempfile,io,gzip
repo=pathlib.Path(__file__).resolve().parents[1]
a=argparse.ArgumentParser();a.add_argument('x86');a.add_argument('arm');a.add_argument('output');a.add_argument('--source',required=True);args=a.parse_args()
assert (repo/'SOURCE_ORIGIN.json').is_file(), 'Run from independent public source, never private workspace'
assert re.fullmatch('[0-9a-f]{40}',args.source)
version=re.search(r'^version = "([0-9.]+)"$',(repo/'Cargo.toml').read_text(),re.M)[1]
out=pathlib.Path(args.output).resolve();out.mkdir(parents=True,exist_ok=True)
assert not list(out.iterdir()), 'Use an empty output directory'
archive=out/f'kairo-{version}.tar.gz'
subprocess.run(['sh',str(repo/'scripts/package-offline.sh'),str(pathlib.Path(args.x86).resolve()),str(pathlib.Path(args.arm).resolve()),str(archive)],check=True)
with tarfile.open(archive) as t:
 members=[(m,t.extractfile(m).read() if m.isfile() else None) for m in t]
for arch,binary in [('x86_64',args.x86),('aarch64',args.arm)]:
 target=f'{arch}-unknown-linux-gnu';name=f'kairo-{version}-{target}'
 raw=pathlib.Path(binary).read_bytes();assert raw[:4]==b'\x7fELF'
 (out/name).write_bytes(raw);(out/name).chmod(0o755)
 with (out/(name+'.tar.gz')).open('wb') as f, gzip.GzipFile(fileobj=f,mode='wb',mtime=0,filename='') as zipped, tarfile.open(fileobj=zipped,mode='w') as t:
  for old,data in members:
   if '/bin/kairo-' in old.name and not old.name.endswith('kairo-'+target):continue
   if old.name==f'kairo-{version}/SHA256SUMS':data=(hashlib.sha256(raw).hexdigest()+'  bin/kairo-'+target+'\n').encode()
   m=tarfile.TarInfo(old.name);m.type=old.type;m.mode=old.mode;m.mtime=1792454400;m.uid=m.gid=0
   if data is not None:m.size=len(data)
   t.addfile(m,io.BytesIO(data) if data is not None else None)
for name in ['install.sh','uninstall.sh']:(out/name).write_bytes((repo/name).read_bytes())
subprocess.run(['python3',str(repo/'scripts/release-sbom.py'),str(out/'sbom.spdx.json')],check=True)
(out/'release-metadata.json').write_text(json.dumps({'source_commit':args.source,'version':version,'published':False,'integrity':'github-https-sha256'},indent=2)+'\n')
(out/'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name+'\n' for p in sorted(out.iterdir())))
print('Two architecture packages, binaries, optional combined archive, installer, SBOM and SHA256SUMS prepared; not published')
