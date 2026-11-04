"""Windows development test: native DLL crash -> worker replacement -> fresh gauge data.

Build connector with --features msfs and desktop-cli first. Requires MSVC + MSFS
2020 SDK headers, no running simulator/hardware. The fake DLL is temporary and
is NEVER included in a user package. RFI_NATIVE_BIN selects release/package bins.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('RFI_NATIVE_BIN', ROOT / 'target/debug')).resolve()


@unittest.skipUnless(os.name == 'nt', 'Windows SDK boundary')
class WorkerRecovery(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix='rfi-worker-test-')
        cls.root = Path(cls.tmp.name)
        vswhere = Path(os.environ['ProgramFiles(x86)']) / 'Microsoft Visual Studio/Installer/vswhere.exe'
        vs = subprocess.check_output([str(vswhere), '-latest', '-products', '*', '-property', 'installationPath'], text=True).strip()
        vcvars = Path(vs) / 'VC/Auxiliary/Build/vcvars64.bat'
        include = Path(os.environ.get('MSFS_SDK', 'C:/MSFS SDK')) / 'SimConnect SDK/include'
        cls.dll = cls.root / 'TEST-ONLY-SimConnect.dll'
        batch = cls.root / 'build.cmd'
        batch.write_text(f'@echo off\ncall "{vcvars}" >nul\nif errorlevel 1 exit /b 1\n'
                         f'cl /nologo /LD /MT /EHsc /std:c++17 /I"{include}" '
                         f'"{ROOT / "tests/fixtures/simconnect_worker_fault.cpp"}" '
                         f'/link /OUT:"{cls.dll}"\n', encoding='utf-8')
        result = subprocess.run(['cmd.exe', '/d', '/c', str(batch)], cwd=cls.root, capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def run_connector(self, malformed=False, multi=False):
        case = self.root / ('malformed' if malformed else 'multi' if multi else 'crash')
        case.mkdir()
        env = os.environ.copy()
        env['RFI_FIXTURE_STATE'] = str(case / 'opens.txt')
        env.pop('RFI_FIXTURE_MALFORMED', None)
        if malformed:
            env['RFI_FIXTURE_MALFORMED'] = '1'
        result = subprocess.run([str(BIN / 'instrument-connector.exe'), 'run',
                                 '--desktop', str(BIN / 'desktop-cli.exe'), '--msfs',
                                 '--dll', str(self.dll), '--duration', '7',
                                 '--writeback', '--log-dir', str(case / 'logs')] + (['--sixpack','--app-status'] if multi else []),
                                env=env, text=True, capture_output=True, timeout=25)
        folder = next((case / 'logs').iterdir())
        self.assertTrue((folder / 'summary.json').exists(),
                        f'Connector exited {result.returncode} (0x{result.returncode & 0xffffffff:08X}) without summary:\n{result.stdout}\n{result.stderr}')
        report = json.loads((folder / 'summary.json').read_text())
        records = [json.loads(line) for f in folder.glob('events-*.jsonl') for line in f.read_text().splitlines()]
        return result, report, records, int((case / 'opens.txt').read_text())

    def test_access_violation_and_failed_reopen_recover_without_restarting_connector(self):
        result, report, records, opens = self.run_connector()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(report['end'], 'stopped')
        self.assertEqual(opens, 3)
        self.assertEqual(report['sessions'], 2)
        self.assertEqual(report['reconnect_attempts'], 2)
        self.assertIn('C0000005', result.stderr)
        identities = [json.loads(line.removeprefix('SIMULATOR ')) for line in result.stderr.splitlines() if line.startswith('SIMULATOR ')]
        self.assertGreaterEqual(len(identities), 2)
        self.assertTrue(all(i['name'] == 'MSFS worker fixture' and i['application_version'] == [1,38,2,0] for i in identities))

        frames = [r['frame'].split() for r in records if 'frame' in r]
        groups = {}
        for frame in frames:
            groups.setdefault(frame[1], []).append(frame)
        self.assertEqual(len(groups), 2)
        for group, expected_alt, expected_qnh in zip(groups.values(), ['4520.000', '12000.000'], ['29.9200', '29.8000']):
            valid = [f for f in group if f[5] == 'running' and f[6] != '-']
            self.assertGreater(len(valid), 10)
            self.assertTrue(all(f[6] == expected_alt and abs(float(f[8]) - float(expected_qnh)) < .0001 for f in valid))
            self.assertEqual(group[-1][5:9], ['disconnected', '-', '-', '-'])
        probes = [r['session_probe'] for r in records if 'session_probe' in r]
        self.assertEqual(len(probes), 2)
        self.assertTrue(all('altitude_quality=valid' in p['source'] for p in probes))

    def test_malformed_native_packet_stops_instead_of_retrying(self):
        result, report, records, opens = self.run_connector(malformed=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(report['end'], 'error')
        self.assertEqual(opens, 1)
        self.assertFalse(any('recovery_wait' in r for r in records))
        frames = [r['frame'].split() for r in records if 'frame' in r]
        self.assertTrue(all(f[6] == '-' for f in frames))
        self.assertEqual(frames[-1][5], 'disconnected')

    def test_sixpack_recovers_all_fields_with_correct_attitude_signs(self):
        result, report, records, opens = self.run_connector(multi=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(opens, 3)
        self.assertEqual(report['sessions'], 2)
        frames = [r['frame'].split() for r in records if 'frame' in r]
        valid = [f for f in frames if f[4] == 'running' and len(f) > 6]
        self.assertGreater(len(valid), 20)
        self.assertEqual(len({f[1] for f in valid}), 2)
        for f in valid:
            fields = {int(v.split(':')[0]): v.split(':')[3] for v in f[5:-1]}
            self.assertEqual(set(fields), set(range(1,11)))
            self.assertEqual(float(fields[4]), 5.)
            self.assertEqual(float(fields[5]), 20.)
            self.assertEqual(float(fields[6]), 120.)
            self.assertEqual(float(fields[7]), 270.)
        self.assertIn('STATUS ', result.stdout)


if __name__ == '__main__':
    unittest.main()
