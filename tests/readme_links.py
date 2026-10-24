"""Validate local bilingual README links/anchors and shell example syntax."""
import pathlib,re,subprocess
repo=pathlib.Path(__file__).resolve().parents[1];count=0
for name in ['README.md','README.en.md']:
 text=(repo/name).read_text()
 for link in re.findall(r'\]\(([^)]+)\)',text):
  if '://' in link or link.startswith('mailto:'):continue
  target,_,anchor=link.partition('#');p=repo/(target or name);assert p.is_file(),(name,link)
  if anchor:
   headings=re.findall(r'^#+\s+(.+)$',p.read_text(),re.M)
   slugs=[re.sub(r'[^\w\-\s]','',h.lower()).replace(' ','-') for h in headings]
   assert anchor in slugs,(name,link,slugs)
  count+=1
 for block in re.findall(r'```sh\n(.*?)```',text,re.S):subprocess.run(['bash','-n'],input=block,text=True,check=True)
 assert '--version 0.2.1' not in text and 'bash -o pipefail' in text
print(f'{count} README local links/anchors and shell example syntax passed')
