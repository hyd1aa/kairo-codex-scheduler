"""The archive scanner must detect nested credentials and private key material."""
import io,pathlib,subprocess,sys,tarfile,tempfile
scanner=pathlib.Path(__file__).resolve().parents[1]/'scripts/check-release-secrets.py'
with tempfile.TemporaryDirectory(prefix='kairo-scan-test-') as td:
 root=pathlib.Path(td)
 for name,data in [('clean.txt',b'ordinary release'),('auth.json',b'{}'),('key.pem',b'-----BEGIN '+b'PRIVATE KEY-----')]:
  inner=io.BytesIO()
  with tarfile.open(fileobj=inner,mode='w:gz') as archive:
   member=tarfile.TarInfo(name);member.size=len(data);archive.addfile(member,io.BytesIO(data))
  path=root/'nested.tar.gz'
  with tarfile.open(path,mode='w:gz') as archive:
   content=inner.getvalue();member=tarfile.TarInfo('inner.tar.gz');member.size=len(content);archive.addfile(member,io.BytesIO(content))
  p=subprocess.run([sys.executable,str(scanner),str(path)],capture_output=True)
  assert (p.returncode==0)==(name=='clean.txt'),name
print('3 nested release secret scan regressions passed')
