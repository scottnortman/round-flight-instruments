"""Offline migration guards: run with python -m unittest discover -s scripts -p test_partition_layout.py."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest

import partition_layout as layout
from flash_test_firmware import validated_segments


def binary(entries):
    records = b''.join(struct.pack('<HBBII16sI', 0x50aa, kind, subtype, offset, size,
                                  name.encode(), flags)
                       for name, kind, subtype, offset, size, flags in entries)
    return (records + b'\xeb\xeb' + b'\xff' * 14 + hashlib.md5(records).digest()).ljust(0xc00, b'\xff')


class LayoutTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.firmware = self.root / 'firmware'
        self.firmware.mkdir()
        self.csv = self.firmware / 'partitions.csv'
        self.csv.write_bytes((Path(__file__).resolve().parents[1] / 'firmware/console/partitions.csv').read_bytes())
        self.table = self.firmware / 'partition-table.bin'
        self.table.write_bytes(binary(layout.EXPECTED))
        (self.firmware / 'bootloader.bin').write_bytes(b'\xe9' + bytes(255))
        (self.firmware / 'application.bin').write_bytes(b'\xe9' + bytes(4095))
        self.manifest()

    def manifest(self):
        (self.root / 'manifest.json').write_text(json.dumps({'files': [
            {'path': 'firmware\\' + p.name, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in self.firmware.iterdir()]}))

    def test_layout_and_nonoverlapping_flash_segments(self):
        self.assertEqual(layout.validate(self.csv, self.table)['app_size'], 4194304)
        segments = validated_segments(self.root)
        self.assertEqual([offset for offset, _ in segments], [0, 0x8000, 0x10000])
        # No padded image spans the persisted 0x9000..0xffff range.
        for offset, path in segments:
            self.assertTrue(offset >= 0x10000 or offset + path.stat().st_size <= 0x9000)

    def test_wrong_app_size_or_changed_persistent_data_locations(self):
        for index, field, value in [(2, 4, 0x100000), (0, 3, 0xa000), (1, 3, 0xe000)]:
            entries = [list(v) for v in layout.EXPECTED]
            entries[index][field] = value
            self.table.write_bytes(binary(entries))
            with self.subTest(index=index), self.assertRaises(ValueError):
                layout.validate(self.csv, self.table)

    def test_corrupt_truncated_missing_digest_or_extra_binary_data(self):
        good = self.table.read_bytes()
        corrupt = bytearray(good)
        corrupt[100] ^= 1
        for data in [good[:-1], bytes(corrupt), good[:96] + b'\xff' * (0xc00 - 96), good[:-1] + b'\0']:
            self.table.write_bytes(data)
            with self.subTest(length=len(data)), self.assertRaises(ValueError):
                layout.validate(self.csv, self.table)

    def test_changed_csv_and_wrong_sdk_path_are_rejected(self):
        sdk = self.root / 'sdkconfig'
        sdk.write_text('CONFIG_PARTITION_TABLE_CUSTOM=y\nCONFIG_PARTITION_TABLE_OFFSET=0x8000\n'
                       f'CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="{self.csv.as_posix()}"\n')
        layout.validate(self.csv, self.table, sdk)
        sdk.write_text(sdk.read_text().replace('partitions.csv', 'other.csv'))
        with self.assertRaises(ValueError):
            layout.validate(self.csv, self.table, sdk)
        self.csv.write_text(self.csv.read_text().replace('0x400000', '0x100000'))
        with self.assertRaises(ValueError):
            layout.validate(self.csv, self.table)

    def test_tampering_and_oversized_images_are_rejected_before_flash(self):
        app = self.firmware / 'application.bin'
        app.write_bytes(b'\xe9modified')
        with self.assertRaises(ValueError):
            validated_segments(self.root)
        app.write_bytes(b'\xe9' + bytes(layout.APP_SIZE))
        self.manifest()
        with self.assertRaises(ValueError):
            validated_segments(self.root)
        app.write_bytes(b'bad image')
        self.manifest()
        with self.assertRaises(ValueError):
            validated_segments(self.root)


if __name__ == '__main__':
    unittest.main()
