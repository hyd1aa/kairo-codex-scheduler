#!/usr/bin/env python3
"""Create deterministic dependency notices from locally downloaded Cargo sources."""
import json,pathlib,subprocess,hashlib
metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--offline','--format-version','1']))
packages=sorted([p for p in metadata['packages'] if p['source']],key=lambda p:(p['name'],p['version']))
rows=['name\tversion\tlicense'];sections=[];missing=[]
for p in packages:
    rows.append(f"{p['name']}\t{p['version']}\t{p.get('license') or 'REVIEW REQUIRED'}")
    root=pathlib.Path(p['manifest_path']).parent
    # Nested vendored components (for example ring/once_cell) retain their notices.
    files=sorted(f for f in root.rglob('*') if f.is_file() and any(f.name.upper().startswith(n) for n in ['LICENSE','COPYING','COPYRIGHT','NOTICE']))
    if not files and p['name']=='r-efi':files=[root/'AUTHORS']
    if not files and p['name'] in ['winapi-i686-pc-windows-gnu','winapi-x86_64-pc-windows-gnu']:files=[pathlib.Path('licenses/winapi-MIT.txt')]
    if not files:missing.append(f"{p['name']} {p['version']}")
    sections.append(f"\n{'='*72}\n{p['name']} {p['version']} — {p.get('license')}\n")
    for f in files:
        label=f.relative_to(root) if f.is_relative_to(root) else f
        sections.append(f"\n--- {label} ---\n{f.read_text(errors='replace')}\n")
pathlib.Path('docs/DEPENDENCIES.tsv').write_text('\n'.join(rows)+'\n')
pathlib.Path('docs/THIRD_PARTY_LICENSES.txt').write_text('Dependency license texts from Cargo registry packages; each retains its own license.\n'+''.join(sections))
pathlib.Path('docs/LICENSE_REVIEW.md').write_text('# Dependency license review\n\n'+('Packages without a top-level license file; manual review remains required:\n\n'+''.join('- '+p+'\n' for p in missing) if missing else 'All dependency packages have bundled license/notice coverage, including r-efi AUTHORS and documented upstream winapi overrides. Maintainer attribution review remains a publication gate.\n'))
print(f'{len(packages)} dependency packages inventoried; {len(missing)} need file review')
