"""Record the exact non-ignored repository files accompanying a local test build."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import zipfile

root = Path(__file__).resolve().parents[1]
target = Path(sys.argv[1]).resolve()
files = subprocess.check_output(['git', 'ls-files', '-c', '-o', '--exclude-standard', '-z'], cwd=root).decode().split('\0')
entries = []
with zipfile.ZipFile(target / 'source-snapshot.zip', 'x', zipfile.ZIP_DEFLATED) as archive:
    for name in sorted(set(files)):
        if not name:
            continue
        path = (root / name).resolve()
        if not path.is_relative_to(root) or not path.is_file():
            raise RuntimeError(f'Cannot snapshot source path {name}')
        data = path.read_bytes()
        archive.writestr(name, data)
        entries.append({'path': name, 'sha256': hashlib.sha256(data).hexdigest()})
feature = sys.argv[2] if len(sys.argv) > 2 else 'sim-bridge/msfs'
metadata = {'files': entries, 'rustc': subprocess.check_output(['rustc', '-Vv'], cwd=root).decode(),
            'firmware_rustc': (None if feature == 'instrument-connector/msfs' else
                               subprocess.check_output(['rustc', '-Vv'], cwd=root / 'firmware/console').decode()),
            'cargo': subprocess.check_output(['cargo', '-V'], cwd=root).decode().strip(),
            'features': [feature],
            'note': 'Source snapshot, not a claim of bit-for-bit reproducible compiler output.'}
(target / 'source-manifest.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
