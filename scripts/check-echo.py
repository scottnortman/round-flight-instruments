"""Verify board character echo with raw UART, then automatic-client negotiation."""
import argparse
import json
from pathlib import Path
import time
import serial
from cli import Link, UART_BAUD, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    result = {'passed': False, 'baud': UART_BAUD, 'exchanges': []}
    port = serial.Serial(port=None, baudrate=UART_BAUD, timeout=0.2, write_timeout=2,
                         xonxoff=False, rtscts=False, dsrdtr=False)
    port.dtr = False
    port.rts = False
    port.port = args.port

    def line():
        data = bytearray()
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            data.extend(port.read_until(b'\n', 1025 - len(data)))
            require(len(data) <= 1024, 'Unbounded console line')
            if data.endswith(b'\n'):
                return bytes(data)
        raise TimeoutError(repr(data))

    def exchange(request):
        port.write(request)
        port.flush()
        lines = []
        for _ in range(8):
            reply = line()
            lines.append(reply.decode('ascii'))
            if reply.startswith((b'OK ', b'ERR ')):
                result['exchanges'].append({'request_hex': request.hex(), 'lines': lines})
                return lines
        raise AssertionError('No reply')

    try:
        port.open()
        require(exchange(b'echo status\n') == ['echo status\r\n', 'OK echo=on\r\n'],
                'Board must start with echo on')
        latencies = []
        for byte in b'status':
            start = time.monotonic()
            port.write(bytes([byte]))
            port.flush()
            echoed = port.read(1)
            require(echoed == bytes([byte]), f'Character not echoed before Enter: {echoed!r}')
            latencies.append(round((time.monotonic() - start) * 1000, 1))
        port.write(b'\r\n')
        require(line() == b'\r\n', 'CRLF must echo one newline')
        require(b'timing_fault=false' in line(), 'Missing healthy status reply')
        require(port.read(1) == b'', 'Duplicate CRLF echo/reply')
        result['character_echo_ms'] = latencies
        require(exchange(b'echo off\n') == ['echo off\r\n', 'OK echo=off\r\n'], 'Disable failed')
        require(exchange(b'echo status\n') == ['OK echo=off\r\n'], 'Echo not disabled')
        require(exchange(b'echo on\r') == ['OK echo=on\r\n'], 'Enable failed')
        require(exchange(b'\x1b[2J\x08\x7f\xff\n') ==
                ['\\x1B[2J\\x08\\x7F\\xFF\r\n', 'ERR invalid_byte\r\n'],
                'Unsupported controls must be escaped and rejected')
        require(exchange(b'x' * 300 + b'\n') ==
                ['x' * 128 + '[...]\r\n', 'ERR line_too_long\r\n'],
                'Overflow echo must be bounded; command must be rejected')
        require(exchange(b'echo status\n')[-1] == 'OK echo=on\r\n', 'Framer did not recover')
        port.close()
        client = Link(args.port)
        try:
            require(client.command('echo status') == 'OK echo=off', 'Client did not disable echo')
            for request in ('instrument altimeter', 'baro 29.92', 'sim preset level', 'sim resume',
                            'display gauge', 'display on'):
                client.command(request, 'OK ')
            result['client_status'] = client.command('status', 'OK status')
            require('timing_fault=false' in result['client_status'], result['client_status'])
        finally:
            client.close()
        port.open()
        require(exchange(b'echo status\n') == ['echo status\r\n', 'OK echo=on\r\n'],
                'Client failed to restore board echo')
        port.close()
        client = Link(args.port)
        client.write(b'sim ')
        client.close()  # Must not append an echo command to an incomplete request.
        port.open()
        require(exchange(b'\n') == ['ERR syntax\r\n'], 'Incomplete input was altered on close')
        require(exchange(b'echo status\n') == ['OK echo=off\r\n'],
                'Interrupted session unexpectedly sent another command')
        require(exchange(b'echo on\n') == ['OK echo=on\r\n'], 'Echo recovery failed')
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        port.close()
        args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
