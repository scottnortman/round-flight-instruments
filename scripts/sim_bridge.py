"""Run a bounded Rust provider against desktop CLI or UART (921600 8N1).

Build: cargo build -p sim-bridge -p desktop-cli --locked
Example: python scripts/sim_bridge.py --synthetic 60 [--port COM5]
On EOF/failure the gauge stays external and becomes invalid; no demo fallback.
"""
import argparse
import json
import math
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
import zlib

from cli import Link, ROOT


def decode_frame(line):
    if len(line) > 256 or not line.isascii():
        raise ValueError('oversized/non-ASCII telemetry')
    body, crc = line.rsplit(' *', 1)
    if len(crc) != 8 or int(crc, 16) != zlib.crc32(body.encode('ascii')):
        raise ValueError('telemetry checksum mismatch')
    fields = body.split(' ')
    if len(fields) != 9 or fields[0] != 'T1':
        raise ValueError('unsupported telemetry profile')
    for i in (1, 2, 3, 4):
        if not fields[i].isascii() or not fields[i].isdigit():
            raise ValueError('invalid telemetry identity/time')
    if fields[5] not in ('running', 'paused', 'loading', 'disconnected'):
        raise ValueError('invalid lifecycle')
    return fields


def stamp_frame(line, receiver_ms, elapsed_ms):
    fields = decode_frame(line)
    fields[3] = str(receiver_ms)
    fields[4] = str(min(2**32 - 1, int(fields[4]) + max(0, elapsed_ms)))
    body = ' '.join(fields)
    return f'{body} *{zlib.crc32(body.encode("ascii")):08X}'


def attributes(response):
    return dict(word.split('=', 1) for word in response.split() if '=' in word)


class ProviderProcess:
    def __init__(self, args, exe=None):
        exe = exe or (ROOT / 'bin/sim-bridge.exe' if (ROOT / 'bin/sim-bridge.exe').is_file()
                      else ROOT / 'target/debug/sim-bridge.exe')
        self.process = subprocess.Popen([str(exe), *args], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE)
        self.lines = queue.Queue(maxsize=1)
        def receive():
            while True:
                line = self.process.stdout.readline(258)
                self.lines.put(line)
                if not line or len(line) > 257:
                    return
        threading.Thread(target=receive, daemon=True).start()

    def read(self, timeout=1):
        try:
            raw = self.lines.get(timeout=timeout)
        except queue.Empty as e:
            raise TimeoutError(f'provider did not reply within {timeout} s') from e
        if not raw:
            raise RuntimeError('provider stopped; see its connection/error message above')
        if len(raw) > 257 or not raw.endswith(b'\n'):
            raise ValueError('invalid provider framing')
        return raw.decode('ascii').rstrip('\r\n')

    def command(self, line):
        self.process.stdin.write(line.encode('ascii') + b'\n')
        self.process.stdin.flush()
        return self.read()

    def poll(self):
        self.process.stdin.write(b'next\n')
        self.process.stdin.flush()
        return self.read()

    def close(self):
        if self.process.poll() is None:
            try:
                self.process.stdin.write(b'quit\n')
                self.process.stdin.flush()
                self.process.wait(timeout=0.5)
            except (BrokenPipeError, OSError, subprocess.TimeoutExpired):
                if self.process.poll() is None:
                    self.process.terminate()
        self.process.wait(timeout=3)
        try:
            self.process.stdin.close()
        except OSError:
            pass
        self.process.stdout.close()


def request(link, line, prefix):
    start = time.monotonic()
    link.write(line.encode('ascii') + b'\n')
    response = link.response(timeout=0.2)
    if not response.startswith(prefix):
        raise RuntimeError(f'{line[:60]}: {response}')
    return response, (time.monotonic() - start) * 1000


def seal_packet(body):
    return f'{body} *{zlib.crc32(body.encode("ascii")):08X}'


def checked_packet(line, version, count):
    if len(line) > 256 or not line.isascii():
        raise ValueError('invalid control length/encoding')
    body, crc = line.rsplit(' *', 1)
    if len(crc) != 8 or int(crc, 16) != zlib.crc32(body.encode('ascii')):
        raise ValueError('control checksum mismatch')
    parts = body.split(' ')
    if len(parts) != count or parts[0] != version:
        raise ValueError('unsupported control packet')
    if version == 'R1' and parts[3] not in ('sent', 'confirmed', 'rejected', 'timeout'):
        raise ValueError('invalid control result')
    return parts


def control_reply(session, request_id, status):
    return seal_packet(f'R1 {session} {request_id} {status}')


def send_control_result(link, packet):
    # A cancelled/expired edit may reject a delayed reply; never retry its write.
    link.write(packet.encode('ascii') + b'\n')
    response = link.response(timeout=0.2)
    if not response.startswith(('OK control result', 'ERR not_ready')):
        raise RuntimeError(f'control result delivery: {response}')
    print(f'{packet.split(" *")[0]}: {response}', flush=True)


def run(args):
    if args.writeback and not args.msfs:
        raise ValueError('--writeback requires the MSFS primary-altimeter profile')
    if args.msfs:
        dll = args.dll or (ROOT / 'SimConnect.dll' if (ROOT / 'SimConnect.dll').is_file()
                           else Path(os.environ.get('MSFS_SDK', r'C:\MSFS SDK')) / 'SimConnect SDK/lib/SimConnect.dll')
        if not dll.is_file():
            raise FileNotFoundError(f'SimConnect.dll not found: {dll}; supply --dll PATH from your MSFS 2020 SDK')
        provider_args = ['msfs', '--dll', str(dll.resolve())]
        if args.writeback:
            provider_args.append('--writeback')
    else:
        provider_args = ['synthetic', str(args.synthetic)] if args.synthetic is not None else ['replay', str(args.replay.resolve())]
    provider = ProviderProcess(provider_args, args.bridge_exe)
    link = None
    log = None
    started = False
    report_written = False
    last_status = None
    try:
        header = provider.read(timeout=15).split()
        expected_provider = 'msfs2020' if args.msfs else 'synthetic' if args.synthetic is not None else 'replay'
        if len(header) != 3 or header[0] != 'PROVIDER' or header[1] != expected_provider:
            raise ValueError('unexpected provider identity')
        session = int(header[2])
        if not 0 < session < 2**64:
            raise ValueError('invalid session')
        if args.report and args.report.exists():
            raise FileExistsError(f'report already exists: {args.report}')
        if args.log:
            args.log.parent.mkdir(parents=True, exist_ok=True)
            log = args.log.open('x', encoding='utf-8')
        link = Link(args.port)
        response, _ = request(link, f'source external {header[1]} {session}', f'OK source={header[1]} ')
        receiver_ms = int(attributes(response)['now_ms'])
        if args.writeback:
            request(link, f'control enable {session}', 'OK control enabled=true')
        last_ack = time.monotonic()
        start = last_ack
        sequence = 0
        frames = 0
        max_rtt = 0
        max_gap = 0
        last_frame = start
        last_diagnostic = start
        started = True
        seen_control = 0
        active_control = None
        print(f'{header[1]} -> {args.port or "desktop"}; writeback={args.writeback} session={session}', flush=True)
        while True:
            poll_at = time.monotonic()
            line = provider.poll()
            if time.monotonic() - last_ack >= 0.1:
                response, _ = request(link, 'source status', f'OK source={header[1]} ')
                last_status = response
                status = attributes(response)
                if int(status['session']) != session or int(status['sequence']) != sequence:
                    raise RuntimeError('source/session changed unexpectedly')
                receiver_ms = int(status['now_ms'])
                last_ack = time.monotonic()
            if line != 'IDLE':
                fields = decode_frame(line)
                if int(fields[1]) != session or int(fields[2]) != sequence + 1:
                    raise ValueError('provider sequence/session mismatch')
                line = stamp_frame(line, receiver_ms, math.ceil((time.monotonic() - poll_at) * 1000))
                response, rtt = request(link, line, f'OK telemetry session={session} seq={sequence + 1} ')
                receiver_ms = int(attributes(response)['now_ms'])
                last_ack = time.monotonic()
                advertised_control = int(attributes(response).get('control', '0'))
                sequence += 1
                frames += 1
                max_rtt = max(max_rtt, rtt)
                max_gap = max(max_gap, (last_ack - last_frame) * 1000)
                last_frame = last_ack
                if log:
                    log.write(json.dumps({'elapsed_ms': round((last_ack - start) * 1000),
                                          'frame': line, 'ack': response}) + '\n')
                    log.flush()
                if fields[5] == 'disconnected':
                    break
                if args.writeback and advertised_control > seen_control:
                    sent_at = time.monotonic()
                    response, _ = request(link, 'control poll', 'OK control ')
                    if response.startswith('OK control C1 '):
                        packet = response.removeprefix('OK control ')
                        parts = checked_packet(packet, 'C1', 6)
                        identity = (int(parts[1]), int(parts[2]))
                        if identity != (session, advertised_control):
                            raise ValueError('control identity changed during poll')
                        seen_control = advertised_control
                        age = int(parts[5]) + math.ceil((time.monotonic() - sent_at) * 1000)
                        if age > 500:
                            result = control_reply(session, seen_control, 'rejected')
                        else:
                            parts[5] = str(age)
                            result = provider.command(seal_packet(' '.join(parts)))
                        reply = checked_packet(result, 'R1', 4)
                        if reply[1:3] != parts[1:3]:
                            raise ValueError('provider control correlation mismatch')
                        if reply[3] == 'sent':
                            active_control = identity
                        else:
                            send_control_result(link, result)
                        if log:
                            log.write(json.dumps({'control_request': packet, 'result': result}) + '\n'); log.flush()
            if active_control:
                result = provider.command('control')
                reply = checked_packet(result, 'R1', 4)
                if (int(reply[1]), int(reply[2])) != active_control:
                    raise ValueError('pending control identity mismatch')
                if reply[3] != 'sent':
                    send_control_result(link, result)
                    if log:
                        log.write(json.dumps({'control_result': result}) + '\n'); log.flush()
                    active_control = None

            if args.diagnostics and time.monotonic() - last_diagnostic >= 1:
                response, _ = request(link, 'source status', f'OK source={header[1]} ')
                last_status = response
                status = attributes(response)
                if int(status['session']) != session or int(status['sequence']) != sequence:
                    raise RuntimeError('source/session changed during diagnostics')
                receiver_ms = int(status['now_ms'])
                last_ack = time.monotonic()
                diagnostic = {'elapsed_s': round(last_ack - start, 2), 'source': response}
                if args.port:
                    diagnostic['display'], _ = request(link, 'display status', 'OK display ')
                    if args.frame_timing:
                        link.write(b'display sample\n')
                        diagnostic['sample'] = link.response(timeout=0.2)
                        if not diagnostic['sample'].startswith(('OK display_sample ', 'ERR display_sample_busy')):
                            raise RuntimeError('display sample unsupported; update firmware or omit --frame-timing')
                diagnostic['query_end_elapsed_s'] = round(time.monotonic() - start, 3)
                print(json.dumps(diagnostic), flush=True)
                if log:
                    log.write(json.dumps({'diagnostic': diagnostic}) + '\n')
                    log.flush()
                last_diagnostic = time.monotonic()
            time.sleep(max(0, 1 / 30 - (time.monotonic() - poll_at)))
        response, _ = request(link, 'source status', f'OK source={header[1]} ')
        elapsed = time.monotonic() - start
        report = {'target': args.port or 'desktop', 'frames': frames,
                  'elapsed_s': round(elapsed, 3), 'accepted_hz': round(frames / elapsed, 2),
                  'max_ack_ms': round(max_rtt, 2), 'max_sample_gap_ms': round(max_gap, 2),
                  'final_status': response}
        print(json.dumps(report, indent=2))
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            with args.report.open('x', encoding='utf-8') as output:
                output.write(json.dumps(report, indent=2) + '\n')
            report_written = True
    finally:
        if started and args.report and not report_written:
            elapsed = time.monotonic() - start
            partial = {'target': args.port or 'desktop', 'frames': frames,
                       'elapsed_s': round(elapsed, 3),
                       'accepted_hz': round(frames / elapsed, 2) if elapsed else 0,
                       'max_ack_ms': round(max_rtt, 2), 'max_sample_gap_ms': round(max_gap, 2),
                       'end': 'interrupted_or_error', 'last_observed_status': last_status,
                       'note': 'Last observation is not a post-stop device status; data will expire.'}
            try:
                args.report.parent.mkdir(parents=True, exist_ok=True)
                with args.report.open('x', encoding='utf-8') as output:
                    output.write(json.dumps(partial, indent=2) + '\n')
            except OSError as error:
                print(f'Could not preserve summary: {error}', flush=True)
        if log:
            log.close()
        try:
            if link:
                link.close()
        finally:
            provider.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--synthetic', type=int, metavar='SECONDS')
    mode.add_argument('--replay', type=Path)
    mode.add_argument('--msfs', action='store_true', help='read-only MSFS 2020 primary altimeter; requires native build')
    parser.add_argument('--dll', type=Path, help='MSFS 2020 SDK SimConnect.dll path')
    parser.add_argument('--bridge-exe', type=Path, help='native executable; defaults to target/debug/sim-bridge.exe')
    parser.add_argument('--port', help='omit for shared desktop CLI')
    parser.add_argument('--log', type=Path, help='new JSONL wire diagnostic log; never overwrites')
    parser.add_argument('--report', type=Path)
    parser.add_argument('--frame-timing', action='store_true', help='include completed-frame provenance with diagnostics; requires updated firmware')
    parser.add_argument('--writeback', action='store_true', help='enable confirmed primary-altimeter barometer requests; requires updated firmware')
    parser.add_argument('--diagnostics', action='store_true', help='sample live source/display status once per second; adds command traffic')
    args = parser.parse_args()
    try:
        run(args)
    except (Exception, KeyboardInterrupt) as error:
        parser.exit(1, f'Bridge stopped: {error}. External data expires; explicitly use source sim to return to demo.\n')


if __name__ == '__main__':
    main()
