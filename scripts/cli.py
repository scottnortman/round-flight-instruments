"""Line-oriented user control and the same acceptance checks on desktop/UART.

Build desktop-cli first, or use the installed IDF Python for --port COM5.
No DTR/RTS reset is requested. --test changes simulated state and ends paused.
"""
import argparse
import json
from pathlib import Path
import queue
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
FIELDS = ('pressure_altitude_ft', 'pitch_deg', 'roll_deg', 'heading_deg',
          'ias_kt', 'turn_dps', 'ball', 'vsi_fpm')
UART_BAUD = 921600


class Link:
    def __init__(self, port, baud=UART_BAUD):
        self.process = None
        self.serial = None
        self.responses = []
        self.device_lines = []
        self.restore_echo = False
        self.in_flight = False
        if port:
            import serial
            self.serial = serial.Serial(port=None, baudrate=baud, timeout=0.2,
                                        bytesize=serial.EIGHTBITS, parity=serial.PARITY_NONE,
                                        stopbits=serial.STOPBITS_ONE, write_timeout=2,
                                        xonxoff=False, rtscts=False, dsrdtr=False)
            self.serial.dtr = False
            self.serial.rts = False
            self.serial.port = port
            self.serial.open()
            try:
                echo = self.command('echo status')
                if echo == 'OK echo=on':
                    self.command('echo off', 'OK echo=off')
                    self.restore_echo = True
                elif echo not in ('OK echo=off', 'ERR syntax'):
                    raise RuntimeError(f'Unexpected echo negotiation: {echo}')
                self.responses.clear()
            except Exception:
                self.serial.close()
                raise
        else:
            self.process = subprocess.Popen([str(ROOT / 'target/debug/desktop-cli.exe')],
                                            stdin=subprocess.PIPE, stdout=subprocess.PIPE)
            self.lines = queue.Queue(maxsize=16)
            def receive():
                while True:
                    line = self.process.stdout.readline(1025)
                    self.lines.put(line)
                    if not line:
                        break
            threading.Thread(target=receive, daemon=True).start()

    def write(self, data):
        self.in_flight = True
        stream = self.serial or self.process.stdin
        stream.write(data)
        stream.flush()

    def response(self, timeout=3, ignored_echo=None):
        deadline = time.monotonic() + timeout
        partial = bytearray()
        while time.monotonic() < deadline:
            if self.serial:
                partial.extend(self.serial.read_until(b'\n', 1025 - len(partial)))
                if len(partial) > 1024:
                    self.device_lines.append(partial[:1024].decode('ascii', errors='backslashreplace'))
                    self.device_lines = self.device_lines[-256:]
                    raise RuntimeError('response exceeds protocol bound')
                if not partial.endswith(b'\n'):
                    continue
                raw = bytes(partial)
                partial.clear()
            else:
                try:
                    raw = self.lines.get(timeout=max(0.001, deadline - time.monotonic()))
                except queue.Empty:
                    break
                if not raw:
                    raise RuntimeError('desktop CLI ended unexpectedly')
            line = raw.decode('ascii', errors='replace').strip()
            if line == ignored_echo:
                continue
            if line.startswith(('OK ', 'ERR ')):
                if len(raw) > 1024 or not raw.endswith(b'\n'):
                    raise RuntimeError('malformed response framing')
                self.in_flight = False
                return line
            if line:
                self.device_lines.append(line)
                self.device_lines = self.device_lines[-256:]
                print(f'[device] {line.encode("ascii", errors="backslashreplace").decode("ascii")}', flush=True)
        raise TimeoutError('no complete CLI response within deadline')

    def command(self, command, prefix=None):
        start = time.monotonic()
        self.write(command.encode('ascii') + b'\n')
        response = self.response(ignored_echo=command.strip())
        self.responses.append({'command': command, 'response': response,
                               'elapsed_ms': round((time.monotonic() - start) * 1000, 1)})
        if prefix is not None and not response.startswith(prefix):
            raise AssertionError(f'{command!r}: expected {prefix!r}, got {response!r}')
        return response

    def close(self):
        if self.serial:
            try:
                # After an interrupted/timed-out request, another command could
                # be mistaken for its late response. Close without further I/O.
                if self.restore_echo and not self.in_flight:
                    self.command('echo on', 'OK echo=on')
            finally:
                self.serial.close()
        elif self.process:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=3)


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def acceptance(link, slow):
    # These are bench simulation commands only, with one outstanding request.
    ok = lambda cmd: link.command(cmd, 'OK ')
    ok('help')
    require('vsi' in ok('instrument list'), 'instrument list incomplete')
    ok('sim resume')
    for field in FIELDS:
        ok(f'fault {field} off')
        ok(f'sim clear {field}')
    state = ok('status')
    require(all(f'{f}=-:missing:none' in state for f in FIELDS), 'missing must not become zero')
    require('timing_fault=false' in state, 'owner timing fault')
    ok('sim preset level')
    ok('sim set ias_kt 0')
    require('ias_kt=0.000:valid:' in ok('status'), 'zero must remain valid')
    for command, error in [('sim set ias_kt NaN', 'number'), ('sim set ias_kt inf', 'number'),
                           ('sim set ias_kt 201', 'range'), ('sim set heading_deg 360', 'range'),
                           ('baro 31.01', 'range'), ('status extra', 'syntax')]:
        link.command(command, f'ERR {error}')
    require('ias_kt=0.000:valid:' in ok('status'), 'rejected input changed state')
    ok('baro 30.12')
    for name in ('altimeter', 'attitude', 'airspeed', 'heading', 'turn', 'vsi'):
        ok(f'instrument {name}')
        state = ok('status')
        require(f'instrument={name}' in state and 'baro_inhg=30.12' in state,
                'view switch changed settings')
    ok('fault pitch_deg on')
    state = ok('status')
    require('pitch_deg=-:failed:' in state and 'ias_kt=0.000:valid:' in state,
            'faults must be independent')
    ok('fault pitch_deg off')
    require('pitch_deg=0.000:valid:' in ok('status'), 'fault did not recover with fresh sample')
    ok('sim pause')
    time.sleep(1.1)
    state = ok('status')
    require(all(f'{f}=-:stale:' in state for f in FIELDS), 'paused data must expire')
    ok('sim resume')
    require('ias_kt=0.000:valid:' in ok('status'), 'resume failed')
    link.write(b' ' * 129 + b'sim pause\n')
    require(link.response() == 'ERR line_too_long', 'overflow was not rejected')
    require('sim=running' in ok('status'), 'overflow suffix executed')
    link.write(b'\xffsim pause\n')
    require(link.response() == 'ERR invalid_byte', 'non-ASCII was not rejected')
    require('sim=running' in ok('status'), 'invalid-byte suffix executed')
    link.write(b'status\r\n')
    require(link.response().startswith('OK status'), 'CRLF failed')
    # Next response must belong to this request, not an extra CRLF response.
    require(ok('instrument attitude').startswith('OK instrument=attitude display='), 'CRLF duplicate')
    if slow:
        print('Checking incomplete-line timeout (30 seconds)...', flush=True)
        link.write(b'sim ')
        time.sleep(30.2)
        link.write(b'pause\n')
        require(link.response() == 'ERR line_timeout', 'partial-line timeout failed')
        require('sim=running' in ok('status'), 'timed-out suffix executed')
    ok('sim pause')
    time.sleep(1.1)
    state = ok('status')
    require('timing_fault=false' in state, 'timing fault during acceptance checks')
    gaps = [int(row['response'].split('max_gap_ms=')[1].split()[0])
            for row in link.responses if row['response'].startswith('OK status')]
    return {'passed': True, 'requests': len(link.responses),
            'max_response_ms': max(r['elapsed_ms'] for r in link.responses),
            'max_owner_gap_ms': max(gaps), 'slow_timeout_test': slow,
            'final_status': state, 'transcript': link.responses}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', help='UART port, e.g. COM5; omitted uses desktop executable')
    parser.add_argument('--baud', type=int, default=UART_BAUD,
                        help='UART baud rate (default 921600; older firmware uses 115200)')
    parser.add_argument('--test', action='store_true', help='run simulation acceptance checks')
    parser.add_argument('--slow', action='store_true', help='include 30-second line timeout')
    parser.add_argument('--report', type=Path, help='write acceptance result and transcript JSON')
    args = parser.parse_args()
    link = Link(args.port, args.baud)
    try:
        if args.test:
            result = acceptance(link, args.slow)
            result['target'] = args.port or 'desktop'
            if args.port:
                result['baud'] = args.baud
            if args.report:
                args.report.parent.mkdir(parents=True, exist_ok=True)
                args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
            print(json.dumps({k: v for k, v in result.items() if k != 'transcript'}, indent=2))
        else:
            print('Instrument CLI. Type help; watch imu for 10 seconds of live diagnostics; exit to close.')
            while True:
                try:
                    command = input('rfi> ').strip()
                except EOFError:
                    break
                if command == 'exit':
                    break
                if command == 'watch imu':
                    print('Polling imu read for 10 seconds; Ctrl+C stops watching. Use imu on first.')
                    try:
                        deadline = time.monotonic() + 10
                        while time.monotonic() < deadline:
                            print(link.command('imu read'))
                            time.sleep(0.1)
                    except KeyboardInterrupt:
                        # Do not interrupt a wire request and then misassociate its
                        # late response with a new command; close the session.
                        break
                    continue
                if command:
                    if len(command.encode('ascii')) > 128:
                        print('Command must be at most 128 ASCII bytes.')
                        continue
                    print(link.command(command))
    finally:
        link.close()


if __name__ == '__main__':
    main()
