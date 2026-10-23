#!/usr/bin/env python3
"""Generate/check both-language error/help catalog from bilingual source literals."""
import json,pathlib,re,sys
root=pathlib.Path(__file__).resolve().parents[1];catalog={}
# Stable precedence for duplicate aliases across files, including archive exports.
for path in sorted((root/'crates').rglob('*.rs')):
    for match in re.finditer(r'"((?:[^"\\]|\\.)*)"',path.read_text()):
        try:s=json.loads('"'+match[1]+'"')
        except json.JSONDecodeError:continue
        if ' / ' not in s:continue
        en,zh=s.split(' / ',1)
        if not re.search('[\u4e00-\u9fff]',zh) or re.search('[\u4e00-\u9fff]',en):continue
        item={'en':en.replace('_',' '),'zh':zh}
        catalog[en]=item;catalog[en.lower().replace(' ','_').replace('/','_')]=item
path=root/'crates/kairo-cli/src/messages.json'
content=json.dumps(catalog,ensure_ascii=False,indent=2,sort_keys=True)+'\n'
if '--check' in sys.argv:
    assert path.read_text()==content,'Translation catalog stale; run scripts/i18n-catalog.py'
    print(f'{len(catalog)} bilingual message aliases checked')
else:path.write_text(content)
