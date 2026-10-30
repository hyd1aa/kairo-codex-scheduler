#!/usr/bin/env python3
"""SPDX2.3 lockfile inventory, with registry checksums and explicit root relations."""
import datetime,hashlib,json,pathlib,subprocess,sys,re
repo=pathlib.Path(__file__).resolve().parents[1];lock=repo/'Cargo.lock';metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--offline','--format-version','1'],cwd=repo)); data={'package':metadata['packages']}; packages=[];relationships=[]
checksums={}
for block in lock.read_text().split('[[package]]')[1:]:
 fields=dict(re.findall(r'^(name|version|checksum) = "([^"]+)"$',block,re.M))
 if 'checksum' in fields:checksums[(fields['name'],fields['version'])]=fields['checksum']
for index,p in enumerate(data['package']):
 sid='SPDXRef-Package-'+str(index)
 package={'SPDXID':sid,'name':p['name'],'versionInfo':p['version'],'downloadLocation':('https://crates.io/api/v1/crates/'+p['name']+'/'+p['version']+'/download') if (p.get('source') or '').startswith('registry+') else 'NOASSERTION','filesAnalyzed':False,'licenseConcluded':'NOASSERTION','licenseDeclared':'NOASSERTION','copyrightText':'NOASSERTION','externalRefs':[{'referenceCategory':'PACKAGE-MANAGER','referenceType':'purl','referenceLocator':'pkg:cargo/'+p['name']+'@'+p['version']}]}
 checksum=checksums.get((p['name'],p['version']))
 if checksum:package['checksums']=[{'algorithm':'SHA256','checksumValue':checksum}]
 if p.get('license'):package['licenseDeclared']=p['license']
 packages.append(package);relationships.append({'spdxElementId':'SPDXRef-DOCUMENT','relationshipType':'DESCRIBES','relatedSpdxElement':sid})
identifiers={p['id']:'SPDXRef-Package-'+str(i) for i,p in enumerate(metadata['packages'])}
for node in metadata['resolve']['nodes']:
 for dep in node['deps']:relationships.append({'spdxElementId':identifiers[node['id']],'relationshipType':'DEPENDS_ON','relatedSpdxElement':identifiers[dep['pkg']]})
report={'spdxVersion':'SPDX-2.3','dataLicense':'CC0-1.0','SPDXID':'SPDXRef-DOCUMENT','name':'Kairo-'+next(p['version'] for p in metadata['packages'] if p['name']=='kairo-cli')+'-Cargo.lock-inventory','documentNamespace':'https://spdx.org/spdxdocs/kairo-'+hashlib.sha256(lock.read_bytes()).hexdigest(),'creationInfo':{'creators':['Tool: Kairo release-sbom.py'],'created':datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')},'comment':'All Cargo.lock packages, including target/dev-only dependencies. This lockfile inventory is not a claim every package is linked into every binary; license review in docs/DEPENDENCIES.tsv and THIRD_PARTY_NOTICES.md.','packages':packages,'relationships':relationships}
pathlib.Path(sys.argv[1]).write_text(json.dumps(report,indent=2)+'\n');print(len(packages),'lockfile packages inventoried')
