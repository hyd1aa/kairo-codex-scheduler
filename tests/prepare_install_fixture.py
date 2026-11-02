"""Single-architecture local installer fixture, never a publishable release."""
import hashlib, io, json, pathlib, re, subprocess, sys, tarfile
repo = pathlib.Path(__file__).resolve().parents[1]
binary = pathlib.Path(sys.argv[1]).resolve()
out = pathlib.Path(sys.argv[2]); out.mkdir(parents=True, exist_ok=False)
version = subprocess.check_output([str(binary), '--version'], text=True).strip().split()[-1]
assert re.fullmatch(r'\d+\.\d+\.\d+', version)
data = binary.read_bytes(); assert data[:4] == b'\x7fELF'
# This acceptance runner is x86_64 only. Real release builds use both native CI runners.
assert data[18:20] == b'\x3e\x00'
prefix = f'kairo-{version}/'; name = f'kairo-{version}-x86_64-unknown-linux-gnu.tar.gz'
with tarfile.open(out/name, 'w:gz') as archive:
    for path in [repo/'install.sh', repo/'uninstall.sh', *sorted((repo/'packaging').glob('*'))]:
        if path.is_file():
            raw = path.read_bytes(); item = tarfile.TarInfo(prefix+str(path.relative_to(repo)))
            item.size = len(raw); item.mode = 0o755 if path.suffix == '.sh' else 0o644
            archive.addfile(item, io.BytesIO(raw))
    entries = [('bin/kairo-x86_64-unknown-linux-gnu', data, 0o755), ('SHA256SUMS', (hashlib.sha256(data).hexdigest()+'  bin/kairo-x86_64-unknown-linux-gnu\n').encode(), 0o644)]
    for path, raw, mode in entries:
        item = tarfile.TarInfo(prefix+path); item.size = len(raw); item.mode = mode
        archive.addfile(item, io.BytesIO(raw))
(out/'release-metadata.json').write_text(json.dumps({'version': version, 'local_install_fixture': True}))
(out/'SHA256SUMS').write_text(hashlib.sha256((out/name).read_bytes()).hexdigest()+'  '+name+'\n')
