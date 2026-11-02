"""Explicit update of the Waveshare SKU 29086 test board using an already built image."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    image = root / 'firmware' / 'round-instruments.bin'
    manifest = json.loads((root / 'manifest.json').read_text(encoding='utf-8-sig'))
    record = next(f for f in manifest['files'] if f['path'].replace('\\', '/') == 'firmware/round-instruments.bin')
    if hashlib.sha256(image.read_bytes()).hexdigest().lower() != record['sha256'].lower():
        raise RuntimeError('Firmware checksum mismatch; do not flash this package')
    print('Updating Waveshare ESP32-S3-Touch-LCD-2.8C, SKU 29086. Close the bridge and terminals first.', flush=True)
    subprocess.run([sys.executable, '-m', 'esptool', '--chip', 'esp32s3', '--port', args.port,
                    '--baud', '460800', 'write-flash', '0x0', str(image)], check=True)
    print('Firmware written. Wait for restart, then run start_msfs.py. CLI remains 921600/8N1.', flush=True)


if __name__ == '__main__':
    main()
