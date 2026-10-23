"""Catalog output must be independent of filesystem enumeration order."""
import pathlib, runpy, unittest.mock
repo=pathlib.Path(__file__).resolve().parents[1]
catalog=repo/'crates/kairo-cli/src/messages.json'
expected=catalog.read_bytes()
original=pathlib.Path.rglob
for reverse in (False,True):
    def reordered(path, pattern):
        return iter(sorted(original(path,pattern), reverse=reverse))
    with unittest.mock.patch.object(pathlib.Path,'rglob',reordered):
        runpy.run_path(str(repo/'scripts/i18n-catalog.py'),run_name='__main__')
    assert catalog.read_bytes()==expected, 'Catalog depends on filesystem traversal order'
print('2 catalog traversal-order regressions passed')
