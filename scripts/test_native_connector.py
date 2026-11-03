"""Development-only executable acceptance. The shipped connector needs no Python.

Build instrument-connector and desktop-cli first. Set RFI_NATIVE_BIN for release.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('RFI_NATIVE_BIN', ROOT / 'target/debug')).resolve()
EXE = BIN / ('instrument-connector.exe' if os.name == 'nt' else 'instrument-connector')
DESKTOP = BIN / ('desktop-cli.exe' if os.name == 'nt' else 'desktop-cli')


class NativeAcceptance(unittest.TestCase):
    def run_session(self, provider, *options):
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run([str(EXE), 'run', '--desktop', str(DESKTOP),
                                     *provider, '--log-dir', tmp, *options],
                                    text=True, capture_output=True, timeout=25)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            folders = list(Path(tmp).iterdir())
            self.assertEqual(len(folders), 1)
            report = json.loads((folders[0] / 'summary.json').read_text())
            records = [json.loads(line) for f in folders[0].glob('events-*.jsonl')
                       for line in f.read_text().splitlines()]
            self.records = records
            frames = [r for r in records if 'frame' in r]
            sessions = {}
            for r in frames:
                body, crc = r['frame'].rsplit(' *', 1)
                self.assertEqual(zlib.crc32(body.encode()), int(crc, 16))
                fields = body.split()
                seq = sessions.get(fields[1], 0) + 1
                sessions[fields[1]] = seq
                self.assertEqual(int(fields[2]), seq)
                self.assertIn(f'session={fields[1]} seq={seq} ', r['ack'])
            self.assertEqual(report['frames'], len(frames))
            self.assertEqual(frames[-1]['frame'].split()[5], 'disconnected')
            self.assertFalse(any('control_request' in r for r in records))
            return report, frames

    def test_synthetic_completes_with_correlated_acknowledgements(self):
        report, frames = self.run_session(['--synthetic', '1'])
        self.assertEqual(report['end'], 'source_disconnected')
        self.assertGreater(len(frames), 10)
        self.assertEqual(frames[-2]['frame'].split()[6], '10000.000')

    def test_lifecycle_replay_keeps_pause_missing_and_failure(self):
        report, frames = self.run_session(['--replay', str(ROOT / 'fixtures/altimeter-lifecycle.txt')], '--diagnostics')
        self.assertEqual(report['end'], 'source_disconnected')
        fields = [r['frame'].split() for r in frames]
        self.assertIn('paused', [f[5] for f in fields])
        self.assertIn('!', [f[6] for f in fields])
        self.assertTrue(any(f[5] == 'running' and f[6] == '-' for f in fields))

    def test_duration_stop_invalidates_without_waiting_for_provider(self):
        report, frames = self.run_session(['--synthetic', '60'], '--duration', '1')
        self.assertEqual(report['end'], 'stopped')
        self.assertEqual(frames[-1]['frame'].split()[6:9], ['-', '-', '-'])

    def test_writeback_cannot_be_enabled_for_synthetic(self):
        result = subprocess.run([str(EXE), 'run', '--desktop', 'does-not-exist',
                                 '--synthetic', '1', '--writeback'],
                                capture_output=True, text=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('writeback requires MSFS', result.stderr)

    def test_switches_create_fresh_sessions_and_recover_after_temporary_unavailability(self):
        report, frames = self.run_session(['--switch-demo'])
        self.assertEqual(report['end'], 'source_disconnected')
        self.assertEqual(report['sessions'], 3)
        self.assertEqual(report['reconnect_attempts'], 3)  # includes one refused open
        # Transition-only evidence must distinguish SDK opening, fresh telemetry,
        # and actual receiver state without per-frame diagnostic queries.
        self.assertEqual(sum('provider_open_begin' in r for r in self.records), 4)
        self.assertEqual(sum('provider_open_end' in r for r in self.records), 4)
        probes = [r['session_probe'] for r in self.records if 'session_probe' in r]
        self.assertEqual(len(probes), 3)
        self.assertEqual(sum('telemetry_ready' in r for r in self.records), 3)
        for probe in probes:
            self.assertIn('altitude_quality=valid', probe['source'])
            self.assertIn(f"session={probe['session']}", probe['source'])
            self.assertGreaterEqual(probe['query_end_elapsed_ms'], probe['elapsed_ms'])
        groups = {}
        for r in frames:
            f = r['frame'].split()
            groups.setdefault(int(f[1]), []).append(f)
        identities = list(groups)
        self.assertEqual(identities, list(range(identities[0], identities[0] + 3)))
        for phase, group in enumerate(groups.values()):
            self.assertEqual(group[0][2], '1')
            self.assertEqual(group[-1][5:9], ['disconnected', '-', '-', '-'])
            self.assertAlmostEqual(float(group[-2][6]), 10000 + phase * 5000, places=2)
            # No writeback flag or intervening user command: each source's QNH
            # replaces the previous setting, along with its own altitude.
            expected = [29.92, 30.01, 29.80][phase]
            self.assertTrue(all(abs(float(f[8]) - expected) < 0.0001
                                for f in group if f[5] == 'running'))

    def test_duration_stops_during_recovery_wait_without_new_data(self):
        # First stream ends around 2s; the intentionally refused connection at
        # around 3s begins a 2s backoff, so 4s stops firmly inside that wait.
        report, frames = self.run_session(['--switch-demo'], '--duration', '4')
        self.assertEqual(report['end'], 'stopped_while_reconnecting')
        self.assertEqual(report['sessions'], 1)
        self.assertEqual(report['reconnect_attempts'], 1)
        self.assertEqual(frames[-1]['frame'].split()[5:9], ['disconnected', '-', '-', '-'])


if __name__ == '__main__':
    unittest.main()
