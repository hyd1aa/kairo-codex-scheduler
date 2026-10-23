"""Focused notice regression using offline metadata; no compilation or downloads."""
import csv,json,pathlib,sys
repo=pathlib.Path(__file__).resolve().parents[1]
metadata=json.loads(pathlib.Path(sys.argv[1]).read_text())
packages=[p for p in metadata['packages'] if p['source']]
rows=list(csv.DictReader((repo/'docs/DEPENDENCY_LICENSE_DECISIONS.tsv').open(),delimiter='\t'))
assert {(p['name'],p['version'],p['license']) for p in packages}=={(p['name'],p['version'],p['declared_license']) for p in rows}
assert all(row['selected_terms'] for row in rows)
text=(repo/'docs/THIRD_PARTY_LICENSES.txt').read_text()
ring=next(p for p in packages if p['name']=='ring')
heading='\n'+'='*72+'\n'+ring['name']+' '+ring['version']+' — '+ring['license']+'\n'
assert text.count(heading)==1
section=text.split(heading,1)[1].split('\n'+'='*72+'\n',1)[0]
root=pathlib.Path(ring['manifest_path']).parent
for name in ('LICENSE-MIT','LICENSE-APACHE'):
 relative=pathlib.Path('src/polyfill/once_cell')/name
 assert '\n--- '+str(relative)+' ---\n' in section
 assert (root/relative).read_text() in section
print(f'{len(rows)} dependency decision rows match locked offline metadata; 2 nested ring notice regressions passed')
