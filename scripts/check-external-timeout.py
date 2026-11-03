"""UART regression: abandoned external handshake, live loading, and recovery.

Uses explicitly synthetic data, requires no MSFS, and restores the simulated
4520-ft altimeter on success. Failure leaves external readings invalid.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import time
import zlib
from cli import Link, require


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    result = dict(passed=False, timestamp_utc=datetime.now(timezone.utc).isoformat())
    link = Link(args.port)
    ok = lambda cmd: link.command(cmd, 'OK ')
    session = time.time_ns() // 1000

    def source():
        return fields(ok('source status'))

    def frame(seq, state='running'):
        credit = source()['now_ms']
        values = '8500 8300 30.12' if state == 'running' else '- - -'
        body = f'T1 {session} {seq} {credit} 0 {state} {values}'
        return f'{body} *{zlib.crc32(body.encode()):08X}'

    def display(quality):
        deadline = time.monotonic() + .6
        while time.monotonic() < deadline:
            d = fields(ok('display status'))
            require(d['failed'] == 'false', str(d))
            if d['shown_quality'] == quality:
                return d
            time.sleep(.025)
        raise AssertionError(f'Display did not reach {quality}: {d}')

    try:
        for cmd in ('source sim', 'display recover', 'display on', 'display gauge',
                    'sim resume', 'instrument altimeter', 'fault pressure_altitude_ft off'):
            ok(cmd)
        ok(f'source external synthetic {session}')
        initial = source()
        require(initial['sequence'] == '0' and initial['altitude_quality'] == 'missing', str(initial))
        display('missing')
        time.sleep(1.15)
        stale = source()
        require(stale['sequence'] == '0' and stale['age_ms'] == 'None', str(stale))
        require(stale['altitude_quality'] == 'stale' and stale['indicated_ft'] == 'None', str(stale))
        require(stale['source'] == 'synthetic', 'Unexpected demo fallback')
        result['abandoned_handshake'] = dict(source=stale, display=display('stale'))

        command = frame(1)
        ok(command)
        valid = display('valid')
        require(valid['shown_alt_ft'] == 'Some(8500.0)' and valid['backlight'] == 'true', str(valid))
        result['first_recovery'] = valid
        require(link.command(command).startswith('ERR '), 'Duplicate telemetry accepted')
        require(source()['sequence'] == '1', 'Rejected duplicate changed sequence')
        time.sleep(1.15)
        require(source()['altitude_quality'] == 'stale', 'Existing sample did not expire')
        display('stale')
        ok(frame(2))
        result['second_recovery'] = display('valid')

        session += 1
        ok(f'source external synthetic {session}')
        require(source()['indicated_ft'] == 'None', 'Previous session value leaked')
        display('missing')
        # Fresh explicit loading frames remain Missing even after the initial
        # handshake deadline. Only an abandoned/no-data link becomes Stale.
        for seq in range(1, 8):
            ok(frame(seq, 'loading'))
            time.sleep(.2)
        loading = source()
        require(loading['state'] == 'loading' and loading['altitude_quality'] == 'missing', str(loading))
        result['live_loading'] = dict(source=loading, display=display('missing'))
        ok(frame(8))
        result['new_session_recovery'] = display('valid')
        require('timing_fault=false' in ok('status'), 'Owner timing fault')
        for cmd in ('source sim', 'sim preset level', 'baro 29.92', 'instrument altimeter'):
            ok(cmd)
        time.sleep(.15)
        final = display('valid')
        require(final['shown_alt_ft'] == 'Some(4520.0)', str(final))
        result.update(passed=True, final_display=final)
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        result['transcript'] = link.responses
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        link.close()
    print(json.dumps({k: v for k, v in result.items() if k != 'transcript'}, indent=2))


if __name__ == '__main__':
    main()
