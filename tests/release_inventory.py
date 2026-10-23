"""Ordinary-file final release gate regression; no installed code or network."""
import hashlib
import importlib.util
import json
import pathlib
import subprocess
import tempfile

repo=pathlib.Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('release_inventory',repo/'scripts/verify-release.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
checks=0
fixture_parent=repo/'sandbox/release-fixtures'
fixture_parent.mkdir(parents=True,exist_ok=True,mode=0o700)
with tempfile.TemporaryDirectory(prefix='kairo-inventory-',dir=fixture_parent) as tmp:
 root=pathlib.Path(tmp);packaging=root/'packaging';packaging.mkdir();assets=root/'assets';assets.mkdir()
 key=root/'fixture.pem';key.touch(mode=0o600)
 def openssl(*args):
  return subprocess.check_output(['openssl',*args],stderr=subprocess.DEVNULL)
 openssl('genpkey','-algorithm','Ed25519','-out',str(key))
 public=openssl('pkey','-in',str(key),'-pubout')
 (packaging/'RELEASE-PUBLIC-KEY.pem').write_bytes(public)
 verifier=(repo/'packaging/release-auth.sh').read_text().replace((repo/'packaging/RELEASE-PUBLIC-KEY.pem').read_text(),public.decode())
 (packaging/'release-auth.sh').write_text(verifier);module.REPO=root
 for name in module.ASSETS:(assets/name).write_bytes(b'fixed harmless release inventory fixture\n')
 source='a'*40
 (assets/'release-metadata.json').write_text(json.dumps({'source_commit':source,'version':'0.2.1'}))
 (assets/'RELEASE-PUBLIC-KEY.pem').write_bytes(public)
 manifest=assets/'kairo-0.2.1.sha256'
 def sign():
  manifest.write_text(''.join(hashlib.sha256((assets/name).read_bytes()).hexdigest()+'  '+name+'\n' for name in module.ASSETS))
  openssl('pkeyutl','-sign','-inkey',str(key),'-rawin','-in',str(manifest),'-out',str(manifest)+'.sig')
  return hashlib.sha256(manifest.read_bytes()).hexdigest()
 digest=sign()
 def check(ok=True,expected_source=source,expected_hash=None):
  global checks
  try:module.verify(assets,expected_source,expected_hash or digest)
  except (ValueError,OSError,subprocess.CalledProcessError):assert not ok
  else:assert ok
  checks+=1
 check();check(False,expected_source='b'*40);check(False,expected_hash='0'*64)
 path=assets/'one-click.sh';original=path.read_bytes();path.write_bytes(b'fixed changed bytes\n');check(False);path.write_bytes(original)
 signature=pathlib.Path(str(manifest)+'.sig');original=signature.read_bytes();signature.write_bytes(b'fixed invalid signature');check(False);signature.write_bytes(original)
 path=assets/'RELEASE-PUBLIC-KEY.pem';path.write_bytes(b'fixed wrong public key');check(False);path.write_bytes(public)
 path=assets/'sbom.spdx.json';original=path.read_bytes();path.unlink();check(False);path.write_bytes(original)
 original=manifest.read_bytes();manifest.write_bytes(original+original.splitlines(keepends=True)[0]);check(False,expected_hash=hashlib.sha256(manifest.read_bytes()).hexdigest());manifest.write_bytes(original)
 check()
print(f'{checks} final release inventory accept/refuse checks passed; no binary execution or publication')
