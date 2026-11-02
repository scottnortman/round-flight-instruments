"""End-to-end portable-core/CLI/relay checks. Build desktop-cli first."""
import os
import time
import unittest
import zlib

from cli import Link
from sim_bridge import attributes, decode_frame, stamp_frame
from sim_bridge import checked_packet, seal_packet, control_reply
from start_msfs import reserve_run
from pathlib import Path
import tempfile
import json
from types import SimpleNamespace
from unittest.mock import patch
from sim_bridge import run


def frame(session, sequence, receiver, state='running', indicated='8500', age=0):
    body = f'T1 {session} {sequence} {receiver} {age} {state} {indicated} 8300 30.120'
    return f'{body} *{zlib.crc32(body.encode()):08X}'


class Integration(unittest.TestCase):
    def test_interruption_preserves_partial_summary_and_existing_reports(self):
        with tempfile.TemporaryDirectory() as tmp:
            args=SimpleNamespace(msfs=False,writeback=False,synthetic=2,replay=None,
                bridge_exe=None,port=None,log=Path(tmp)/'frames.jsonl',report=Path(tmp)/'summary.json',
                diagnostics=False,frame_timing=False)
            with patch('sim_bridge.ProviderProcess.poll',side_effect=KeyboardInterrupt):
                with self.assertRaises(KeyboardInterrupt):run(args)
            summary=json.loads(args.report.read_text())
            self.assertEqual(summary['end'],'interrupted_or_error')
            original=args.report.read_bytes()
            with self.assertRaises(FileExistsError):run(args)
            self.assertEqual(args.report.read_bytes(),original)

    def test_unique_capture_folders_preserve_existing_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            a=reserve_run(Path(tmp)); (a/'console.txt').write_text('original')
            b=reserve_run(Path(tmp))
            self.assertNotEqual(a,b)
            self.assertEqual((a/'console.txt').read_text(),'original')

    def test_board_control_is_pending_until_correlated_confirmed_readback(self):
        link=Link(None)
        try:
            session=int(time.time()*1_000_000)
            ok=lambda s:link.command(s,'OK ')
            status=attributes(ok(f'source external msfs2020 {session}'))
            ok(frame(session,1,int(status['now_ms'])))
            self.assertTrue(link.command('baro 29.92').startswith('ERR unsupported'))
            ok(f'control enable {session}')
            self.assertIn('pending=true',ok('baro 29.92'))
            packet=ok('control poll').removeprefix('OK control ')
            parts=checked_packet(packet,'C1',6)
            self.assertEqual(parts[1:5],[str(session),'1','0','2992'])
            self.assertIn('baro_inhg=Some(30.12)',ok('source status'))
            result=control_reply(session,1,'confirmed')
            self.assertTrue(link.command(result).startswith('ERR not_ready'))
            status=attributes(ok('source status'))
            line=frame(session,2,int(status['now_ms']))
            body=line.split(' *')[0].replace('30.120','29.921')
            ok(seal_packet(body));ok(result)
            self.assertIn('status=confirmed',ok('control poll'))
            self.assertIn('baro_inhg=Some(29.921)',ok('source status'))
            with self.assertRaises(ValueError):checked_packet(result.replace('confirmed','rejected'),'R1',4)
        finally:link.close()

    def test_serial_chunk_reader_preserves_adjacent_lines_and_bounds_input(self):
        class Serial:
            def __init__(self, data): self.data = bytearray(data)
            @property
            def in_waiting(self): return len(self.data)
            def read(self, n):
                out = bytes(self.data[:n]); del self.data[:n]; return out
        link = Link.__new__(Link)
        link.serial = Serial(b'echo\r\nOK first\nOK second\r\n')
        link.serial_pending = bytearray()
        link.device_lines = []
        self.assertEqual(link.response(ignored_echo='echo'), 'OK first')
        self.assertEqual(link.response(), 'OK second')
        link.serial = Serial(b'x' * 1025)
        with self.assertRaisesRegex(RuntimeError, 'exceeds protocol bound'): link.response()

    def test_checksum_age_and_bounded_frame(self):
        f = frame(1, 1, 0, age=12)
        fields = decode_frame(stamp_frame(f, 42, 8))
        self.assertEqual(fields[3:5], ['42', '20'])
        with self.assertRaises(ValueError):
            decode_frame(f.replace('8500', '8501'))
        with self.assertRaises(ValueError):
            decode_frame('x' * 257)

    def test_actual_console_sessions_stale_partial_lines_and_read_only(self):
        link = Link(os.environ.get("RFI_TEST_PORT"))
        try:
            session = int(time.time() * 1_000_000)
            ok = lambda s: link.command(s, 'OK ')
            status = attributes(ok(f'source external replay {session}'))
            stamp = int(status['now_ms'])
            ok(frame(session, 1, stamp))
            self.assertIn('indicated_ft=Some(8500.0)', ok('source status'))
            self.assertTrue(link.command('baro 29.92').startswith('ERR unsupported'))
            self.assertTrue(link.command('sim preset level').startswith('ERR unsupported'))
            self.assertTrue(link.command(frame(session, 1, stamp)).startswith('ERR '))
            status = attributes(ok('source status'))
            bad = frame(session, 2, int(status['now_ms'])).replace('8500', '8501')
            self.assertEqual(link.command(bad), 'ERR checksum')
            status = attributes(ok('source status'))
            delayed = frame(session, 2, int(status['now_ms']))
            link.write(delayed[:20].encode())
            time.sleep(0.3)
            link.write(delayed[20:].encode() + b'\n')
            self.assertTrue(link.response().startswith('ERR not_ready'))
            self.assertEqual(attributes(ok('source status'))['sequence'], '1')
            # Status traffic keeps the link alive, but must not refresh measurements.
            end = time.monotonic() + 1.1
            while time.monotonic() < end:
                ok('source status')
                time.sleep(0.05)
            self.assertIn('altitude_quality=stale', ok('source status'))
            self.assertIn('timing_fault=false', ok('status'))
            for seq, state, expected in [(2, 'paused', 'missing'), (3, 'running', 'valid'), (4, 'disconnected', 'missing')]:
                status = attributes(ok('source status'))
                ok(frame(session, seq, int(status['now_ms']), state))
                self.assertIn(f'altitude_quality={expected}', ok('source status'))
            status = attributes(ok('source status'))
            self.assertTrue(link.command(frame(session, 5, int(status['now_ms']))).startswith('ERR '))
            ok('source sim')
            self.assertTrue(link.command(f'source external replay {session}').startswith('ERR '))
            ok(f'source external replay {session + 1}')
            self.assertIn('altitude_quality=missing', ok('source status'))
        finally:
            link.close()


if __name__ == '__main__':
    unittest.main()
