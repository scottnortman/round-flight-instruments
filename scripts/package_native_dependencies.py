"""Package unchanged registry source archives and notices for the native test ZIP."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

target = Path(sys.argv[1]).resolve()
metadata = json.loads(subprocess.check_output([
    'cargo', 'metadata', '--format-version', '1', '--locked',
    '--filter-platform', 'x86_64-pc-windows-msvc',
    '--features', 'instrument-connector/msfs']))
packages = {p['id']: p for p in metadata['packages']}
nodes = {p['id']: p for p in metadata['resolve']['nodes']}
pending = [p['id'] for p in packages.values() if p['name'] in ('instrument-connector', 'desktop-cli', 'instrument-tray')]
seen = set()
while pending:
    identity = pending.pop()
    if identity in seen:
        continue
    seen.add(identity)
    pending.extend(d['pkg'] for d in nodes[identity]['deps']
                   if any(k['kind'] != 'dev' for k in d['dep_kinds']))
notices = target / 'dependency-sources'
notices.mkdir()
entries = []
for identity in sorted(seen):
    package = packages[identity]
    if not package['source']:
        continue  # Project crates are in source-snapshot.zip.
    source = Path(package['manifest_path']).parent
    # Cargo registry src/<registry>/<package> corresponds to cache/<registry>/<package>.crate.
    archive = source.parents[2] / 'cache' / source.parent.name / (source.name + '.crate')
    if not archive.is_file():
        raise RuntimeError(f'Unchanged dependency source archive missing: {archive}')
    shutil.copyfile(archive, notices / archive.name)
    license_files = [p for p in source.iterdir() if p.is_file()
                     and p.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE'))]
    folder = notices / source.name
    folder.mkdir()
    for p in license_files:
        # Cargo sources can carry pre-1980 mtimes, outside ZIP's timestamp range.
        # Preserve bytes, using the new file's current timestamp for packaging.
        shutil.copyfile(p, folder / p.name)
    entries.append({'name': package['name'], 'version': package['version'],
                    'license': package['license'], 'repository': package['repository'],
                    'source_archive': archive.name})
(target / 'DEPENDENCIES.json').write_text(json.dumps(entries, indent=2) + '\n', encoding='utf-8')
print(f'Packaged {len(entries)} unchanged dependency sources and license notices')
