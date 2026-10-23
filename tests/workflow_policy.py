"""Local configuration gate; does not claim a hosted Actions run."""
import pathlib,re,yaml
repo=pathlib.Path(__file__).resolve().parents[1];workflow=yaml.load((repo/'.github/workflows/release.yml').read_text(),Loader=yaml.BaseLoader)
assert workflow['permissions']=={'contents':'read'}
assert set(workflow['on'])=={'workflow_dispatch'}
assert workflow['on']['workflow_dispatch']['inputs']['sign_candidate']['default']=='false'
assert len(workflow['jobs']['build']['strategy']['matrix']['include'])==2
assert workflow['jobs']['sign']['environment']=='release-signing' and 'refs/heads/main' in workflow['jobs']['sign']['if']
for job in workflow['jobs'].values():
 for step in job['steps']:
  if 'uses' in step:assert re.fullmatch(r'[\w.-]+/[\w.-]+@[0-9a-f]{40}',step['uses'])
  assert 'gh release' not in step.get('run','') and 'git push' not in step.get('run','')
  if step.get('uses','').startswith('actions/checkout@'):assert step['with']['persist-credentials']=='false'
print('Workflow dispatch/read-only/pinned-actions/protected-signing gates passed; hosted CI not run')

publish=yaml.load((repo/'.github/workflows/publish.yml').read_text(),Loader=yaml.BaseLoader)
assert set(publish['on'])=={'workflow_dispatch'}
assert publish['on']['workflow_dispatch']['inputs']['approved']['default']=='false'
assert publish['permissions']=={'contents':'read','actions':'read'}
job=publish['jobs']['draft']
assert job['environment']=='public-release' and 'refs/heads/main' in job['if'] and 'inputs.approved' in job['if']
assert job['permissions']=={'contents':'write','actions':'read'}
commands='\n'.join(step.get('run','') for step in job['steps'])
assert '--draft' in commands and '--verify-tag' in commands and 'verify-release.py' in commands
assert 'git push' not in commands and 'release edit' not in commands and 'git tag ' not in commands
assert 'SOURCE_COMMIT' in commands and 'MANIFEST_SHA256' in commands
for step in job['steps']:
 if 'uses' in step:assert re.fullmatch(r'[\w.-]+/[\w.-]+@[0-9a-f]{40}',step['uses'])
 if step.get('uses','').startswith('actions/checkout@'):assert step['with']['persist-credentials']=='false'
print('Future draft workflow: explicit approval/protected environment/exact-source/manifest/tag/draft-only gates passed')

ci=yaml.load((repo/'.github/workflows/ci.yml').read_text(),Loader=yaml.BaseLoader)
assert ci['permissions']=={'contents':'read'}
for job in ci['jobs'].values():
 assert job['env']['KAIRO_OFFLINE']=='1' and job['env']['CARGO_BUILD_JOBS']=='1'
 for step in job['steps']:
  if 'uses' in step:assert re.fullmatch(r'[\w.-]+/[\w.-]+@[0-9a-f]{40}',step['uses'])
  if step.get('uses','').startswith('actions/checkout@'):assert step['with']['persist-credentials']=='false'
updates=yaml.load((repo/'.github/dependabot.yml').read_text(),Loader=yaml.BaseLoader)
assert {row['package-ecosystem'] for row in updates['updates']}=={'cargo','github-actions'}
print('CI pin/offline/one-job/read-only and dependency-update configuration gates passed')
