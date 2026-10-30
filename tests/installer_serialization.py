"""Check persisted maintenance functions without executing any installer action."""
import pathlib,subprocess,tempfile
repo=pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='kairo-shell-parse-') as temp:
 root=pathlib.Path(temp)
 for filename,function in [('install.sh','kairo_install_main'),('uninstall.sh','kairo_uninstall_main')]:
  source=(repo/filename).read_text();invocation=f'{function} "$@" </dev/null'
  assert source.count(invocation)>=1
  definitions=root/filename
  definitions.write_text(source[:source.rindex(invocation)])
  serialized=subprocess.check_output(['bash','--noprofile','--norc','-c','source "$1"; declare -f "$2"','serialization-check',str(definitions),function],text=True)
  saved=root/('saved-'+filename);saved.write_text(serialized+'\n')
  p=subprocess.run(['bash','-n',str(saved)],text=True,capture_output=True)
  assert p.returncode==0,(filename,p.stderr)
print('Both persisted installer/uninstaller functions parse without executing installation or service commands')
