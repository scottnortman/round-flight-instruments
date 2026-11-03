"""Validate SKU 29086's single-app flash layout before building or flashing.

The CSV is canonical; the independently generated IDF binary must agree with it.
Only the bootloader, partition sector and application are written, preserving NVS.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import struct

FLASH_SIZE = 16 * 1024 * 1024
TABLE_OFFSET = 0x8000
APP_OFFSET = 0x10000
APP_SIZE = 4 * 1024 * 1024
EXPECTED = [('nvs', 1, 2, 0x9000, 0x6000, 0),
            ('phy_init', 1, 1, 0xf000, 0x1000, 0),
            ('factory', 0, 0, APP_OFFSET, APP_SIZE, 0)]


def read_csv(path):
    entries = []
    for row in csv.reader(Path(path).read_text(encoding='utf-8-sig').splitlines()):
        if not row or row[0].lstrip().startswith('#'):
            continue
        name, kind, subtype, offset, size, flags = [v.strip() for v in row]
        entries.append((name, {'app': 0, 'data': 1}[kind],
                        {'nvs': 2, 'phy': 1, 'factory': 0}[subtype],
                        int(offset, 0), int(size, 0), 0 if not flags else int(flags, 0)))
    if entries != EXPECTED:
        raise ValueError('CSV must preserve NVS/PHY and provide the 4 MiB factory app')
    return entries


def read_binary(path):
    data = Path(path).read_bytes()
    if len(data) != 0xc00:
        raise ValueError('Expected IDF 3072-byte partition binary')
    entries = []
    for pos in range(0, len(data), 32):
        record = data[pos:pos + 32]
        magic = struct.unpack_from('<H', record)[0]
        if magic == 0xebeb:
            if record[2:16] != b'\xff' * 14 or record[16:] != hashlib.md5(data[:pos]).digest():
                raise ValueError('Partition MD5 mismatch')
            if data[pos + 32:] != b'\xff' * (len(data) - pos - 32):
                raise ValueError('Unexpected data after partition digest')
            return entries
        if magic != 0x50aa:
            raise ValueError('Invalid partition record or missing digest')
        _, kind, subtype, offset, size, label, flags = struct.unpack('<HBBII16sI', record)
        entries.append((label.rstrip(b'\0').decode('ascii'), kind, subtype, offset, size, flags))
    raise ValueError('Missing partition digest')


def validate(csv_path, binary_path, sdkconfig=None):
    entries = read_csv(csv_path)
    if read_binary(binary_path) != entries:
        raise ValueError('Generated partition binary differs from canonical CSV')
    if sdkconfig:
        config = dict(line.split('=', 1) for line in Path(sdkconfig).read_text().splitlines()
                      if line.startswith('CONFIG_') and '=' in line)
        if config.get('CONFIG_PARTITION_TABLE_CUSTOM') != 'y' or config.get('CONFIG_PARTITION_TABLE_OFFSET') != '0x8000':
            raise ValueError('SDK did not select the custom partition table at 0x8000')
        actual = config.get('CONFIG_PARTITION_TABLE_CUSTOM_FILENAME', '').strip('"')
        if Path(actual).resolve() != Path(csv_path).resolve():
            raise ValueError('SDK custom partition filename differs from canonical CSV')
    return {'flash_size': FLASH_SIZE, 'table_offset': TABLE_OFFSET,
            'app_offset': APP_OFFSET, 'app_size': APP_SIZE}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--csv', required=True, type=Path)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--sdkconfig', type=Path)
    args = parser.parse_args()
    print(json.dumps(validate(args.csv, args.binary, args.sdkconfig)))


if __name__ == '__main__':
    main()
