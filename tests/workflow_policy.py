"""Local workflow policy; does not claim a hosted run."""
import pathlib,re,yaml
repo=pathlib.Path(__file__).resolve().parents[1]
def load(name):return yaml.load((repo/'.github/workflows'/name).read_text(),Loader=yaml.BaseLoader)
release=load('release.yml');assert release['permissions']=={'contents':'read'} and set(release['on'])=={'workflow_dispatch'}
assert 'sign' not in release['jobs'] and len(release['jobs']['build']['strategy']['matrix']['include'])==2
for job in release['jobs'].values():
 for step in job['steps']:
  if 'uses' in step:assert re.fullmatch(r'[\w.-]+/[\w.-]+@[0-9a-f]{40}',step['uses'])
  if step.get('uses','').startswith('actions/checkout@'):assert step['with']['persist-credentials']=='false'
  assert 'gh release' not in step.get('run','') and 'git push' not in step.get('run','')
publish=load('publish.yml');assert set(publish['on'])=={'workflow_dispatch'} and publish['on']['workflow_dispatch']['inputs']['approved']['default']=='false'
job=publish['jobs']['draft'];assert 'inputs.approved' in job['if'] and job['permissions']=={'contents':'write','actions':'read'}
commands='\n'.join(s.get('run','') for s in job['steps']);assert '--draft' in commands and '--verify-tag' in commands and 'sha256sum -c' in commands and 'git push' not in commands
assert 'KAIRO_RELEASE_PRIVATE_KEY' not in (repo/'.github/workflows/release.yml').read_text()
ci=load('ci.yml');assert ci['permissions']=={'contents':'read'}
assert all(j['env']['KAIRO_OFFLINE']=='1' and j['env']['CARGO_BUILD_JOBS']=='1' for j in ci['jobs'].values())
print('Read-only dual-architecture SHA256 preparation, pinned actions, no private signing keys, manual draft-only publication gates passed')
