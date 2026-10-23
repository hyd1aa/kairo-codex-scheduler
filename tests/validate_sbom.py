"""Validate SPDX syntax and complete locked-package/checksum/relationship coverage."""
import json,pathlib,sys,re,jsonschema
bom=json.loads(pathlib.Path(sys.argv[1]).read_text());schema=json.loads(pathlib.Path(sys.argv[2]).read_text())
errors=[e.message for e in jsonschema.Draft7Validator(schema).iter_errors(bom)];assert not errors,errors[:5]
repo=pathlib.Path(__file__).resolve().parents[1];locked={}
for block in (repo/'Cargo.lock').read_text().split('[[package]]')[1:]:
 fields=dict(re.findall(r'^(name|version|checksum) = "([^"]+)"$',block,re.M));locked[(fields['name'],fields['version'])]=fields.get('checksum')
assert {(p['name'],p['versionInfo']) for p in bom['packages']}==set(locked)
ids={p['SPDXID'] for p in bom['packages']};assert len(ids)==len(locked)
for package in bom['packages']:
 checksum=locked[(package['name'],package['versionInfo'])]
 if checksum:assert {'algorithm':'SHA256','checksumValue':checksum} in package['checksums']
for relation in bom['relationships']:assert relation['spdxElementId'] in ids|{'SPDXRef-DOCUMENT'} and relation['relatedSpdxElement'] in ids
assert any(r['relationshipType']=='DEPENDS_ON' for r in bom['relationships'])
print(f"SPDX2.3 schema and {len(locked)} locked packages/checksums/relationships validated")
