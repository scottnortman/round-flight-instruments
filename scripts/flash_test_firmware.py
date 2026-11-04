"""Explicit update of the Waveshare SKU 29086 test board using an already built image."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from partition_layout import validate, APP_OFFSET, APP_SIZE, TABLE_OFFSET


def validated_segments(root):
    """Validate the entire package before any serial port is opened."""
    manifest = json.loads((root / 'manifest.json').read_text(encoding='utf-8-sig'))
    firmware = root / 'firmware'
    names = ['bootloader.bin', 'partition-table.bin', 'application.bin', 'partitions.csv']
    for name in names:
        records = [f for f in manifest['files'] if f['path'].replace('\\', '/') == 'firmware/' + name]
        if len(records) != 1 or hashlib.sha256((firmware / name).read_bytes()).hexdigest().lower() != records[0]['sha256'].lower():
            raise ValueError('Firmware manifest missing, duplicated or checksum mismatch: ' + name)
    validate(firmware / 'partitions.csv', firmware / 'partition-table.bin')
    for name, limit in [('bootloader.bin', TABLE_OFFSET), ('application.bin', APP_SIZE)]:
        data = (firmware / name).read_bytes()
        if not data or data[0] != 0xe9 or len(data) > limit:
            raise ValueError('Invalid or oversized firmware segment: ' + name)
    return [(0, firmware / 'bootloader.bin'),
            (TABLE_OFFSET, firmware / 'partition-table.bin'),
            (APP_OFFSET, firmware / 'application.bin')]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    segments = validated_segments(root)
    print('Updating Waveshare ESP32-S3-Touch-LCD-2.8C, SKU 29086. Close the bridge and terminals first.', flush=True)
    # Underscore spelling supports esptool 4.x and its 5.x compatibility alias.
    subprocess.run([sys.executable, '-m', 'esptool', '--chip', 'esp32s3', '--port', args.port,
                    '--baud', '460800', 'write_flash'] +
                   [v for offset, image in segments for v in (hex(offset), str(image))], check=True)
    print('Firmware written. Wait for restart, then start Round Instruments or your compatible connector. CLI remains 921600/8N1.', flush=True)


if __name__ == '__main__':
    main()
