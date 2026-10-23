"""Verify local Markdown links in the actual release archive, without extraction."""
import pathlib,posixpath,re,sys,tarfile
with tarfile.open(sys.argv[1]) as archive:
 members=[m for m in archive if m.isfile()]
 binaries=[m for m in members if "/bin/kairo-" in m.name]
 assert len(binaries)==2 and all((m.mode & 0o111)==0o111 for m in binaries),"Both bundled binaries must be executable even after artifact mode loss"
 assert all((m.mode & 0o111)==0o111 for m in members if "/packaging/" in m.name and m.name.endswith(".sh")),"Bundled installers must be executable"
 files={m.name:archive.extractfile(m).read() for m in members}
 count=0;missing=[]
 for name,data in files.items():
  if not name.endswith('.md'):continue
  for link in re.findall(r'\]\(([^)]+)\)',data.decode(errors='replace')):
   link=link.split('#',1)[0]
   if not link or '://' in link or link.startswith('mailto:'):continue
   target=posixpath.normpath(posixpath.join(posixpath.dirname(name),link))
   count+=1
   if target not in files:missing.append((name,link))
 if missing:raise AssertionError(missing)
 print(f'{count} bundled documentation links resolve; both binary and installer executable modes verified')
