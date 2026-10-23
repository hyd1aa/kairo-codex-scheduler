"""Cryptographic ordering tests; disposable keys only, no services/models."""
import pathlib,tempfile,subprocess,re,hashlib,shutil,os,sys
repo=pathlib.Path(__file__).resolve().parents[1]
def call(*args,ok=True):
 p=subprocess.run(args,capture_output=True,text=True)
 assert (p.returncode==0)==ok,(args[0],p.returncode,p.stderr[-300:])
 return p
fixture_parent=repo/'sandbox/release-fixtures'
fixture_parent.mkdir(parents=True,exist_ok=True,mode=0o700)
with tempfile.TemporaryDirectory(prefix='kairo-release-trust-',dir=fixture_parent) as tmp:
 root=pathlib.Path(tmp);assets=root/'assets';assets.mkdir();scripts=root/'scripts';scripts.mkdir()
 key=root/'private.pem';key.touch(mode=0o600);call('openssl','genpkey','-algorithm','Ed25519','-out',str(key));pub=call('openssl','pkey','-in',str(key),'-pubout').stdout
 other=root/'wrong.pem';other.touch(mode=0o600);call('openssl','genpkey','-algorithm','Ed25519','-out',str(other));wrongpub=call('openssl','pkey','-in',str(other),'-pubout').stdout
 official=(repo/'packaging/RELEASE-PUBLIC-KEY.pem').read_text()
 for name in ('release-auth.sh','bootstrap-signed.sh'):
  source=(repo/'packaging'/name).read_text();assert official in source
  (scripts/name).write_text(source.replace(official,pub).replace('trust_dir=/opt/kairo/trust','trust_dir='+str(root/'trust')))
 shutil.copyfile(repo/'packaging/signed-update.sh',scripts/'signed-update.sh')
 marker=root/'called';oneclick=('#!/bin/sh\nprintf called >'+str(marker)+'\n').encode()
 (assets/'one-click.sh').write_bytes(oneclick);(assets/'kairo-0.2.1.tar.gz').write_bytes(b'authenticated archive test fixture')
 architecture='aarch64' if os.uname().machine=='aarch64' else 'x86_64'
 asset='kairo-0.2.1-'+architecture+'-unknown-linux-gnu'
 binary_path=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else repo/'target/release/kairo'
 assert call(str(binary_path.resolve()),'--version').stdout.strip()=='kairo 0.2.1'
 harmless_binary=binary_path.read_bytes();(assets/asset).write_bytes(harmless_binary)
 for name in ('release-auth.sh','signed-update.sh'):shutil.copyfile(scripts/name,assets/name)
 manifest=assets/'kairo-0.2.1.sha256';manifest.write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name+'\n' for p in sorted(assets.iterdir())))
 signature=pathlib.Path(str(manifest)+'.sig');call('openssl','pkeyutl','-sign','-inkey',str(key),'-rawin','-in',str(manifest),'-out',str(signature));signed=signature.read_bytes();original=manifest.read_bytes()
 auth=lambda ok=True:call('sh',str(scripts/'release-auth.sh'),'0.2.1',str(assets),asset,ok=ok)
 auth();(assets/asset).write_bytes(b'tampered');auth(False);(assets/asset).write_bytes(harmless_binary)
 manifest.write_bytes(original+b'changed');auth(False);manifest.write_bytes(original)
 verifier=(scripts/'release-auth.sh').read_text();(scripts/'release-auth.sh').write_text(verifier.replace(pub,wrongpub));auth(False);(scripts/'release-auth.sh').write_text(verifier)
 bootstrap=lambda ok=True:call('sh',str(scripts/'bootstrap-signed.sh'),'0.2.1','--source-dir',str(assets),ok=ok)
 bootstrap();assert marker.exists();marker.unlink()
 (root/'trust').chmod(0o777);bootstrap(False);assert not marker.exists();(root/'trust').chmod(0o755)
 (assets/'one-click.sh').write_bytes(oneclick+b'bad');bootstrap(False);assert not marker.exists();(assets/'one-click.sh').write_bytes(oneclick)
 archive=(assets/'kairo-0.2.1.tar.gz').read_bytes();(assets/'kairo-0.2.1.tar.gz').write_bytes(b'bad');bootstrap(False);assert not marker.exists();(assets/'kairo-0.2.1.tar.gz').write_bytes(archive)
 source=(scripts/'bootstrap-signed.sh').read_text();(scripts/'bootstrap-signed.sh').write_text(source.replace(pub,wrongpub));bootstrap(False);assert not marker.exists();(scripts/'bootstrap-signed.sh').write_text(source)
 prefix=root/'prefix';binary=prefix/'current/bin/kairo';binary.parent.mkdir(parents=True);binary.write_bytes(oneclick);binary.chmod(0o755)
 upgrade=lambda ok=True:call('sh',str(scripts/'signed-update.sh'),'0.2.1',str(assets),str(prefix),str(root/'data'),ok=ok)
 upgrade();assert marker.exists();marker.unlink();signature.write_bytes(b'bad');upgrade(False);assert not marker.exists();signature.write_bytes(signed)
 (scripts/'release-auth.sh').write_text(verifier.replace(pub,wrongpub));upgrade(False);assert not marker.exists()
 (scripts/'release-auth.sh').write_text(verifier)
 # Sign legitimate current bytes under a different declared version; no historical binary required.
 newer_asset=asset.replace('0.2.1','0.2.2')
 (assets/newer_asset).write_bytes(harmless_binary)
 newer_manifest=assets/'kairo-0.2.2.sha256'
 newer_manifest.write_text(hashlib.sha256(harmless_binary).hexdigest()+'  '+newer_asset+'\n')
 call('openssl','pkeyutl','-sign','-inkey',str(key),'-rawin','-in',str(newer_manifest),'-out',str(newer_manifest)+'.sig')
 call('sh',str(scripts/'signed-update.sh'),'0.2.2',str(assets),str(prefix),str(root/'data'),ok=False)
 assert not marker.exists()
print('13 pinned-key signature/bootstrap/update ordering checks passed; zero model requests')
