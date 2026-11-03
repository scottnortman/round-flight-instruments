"""UART attitude validity, independent blanking and animated presentation budget.

Leaves standalone attitude level and valid. No MSFS or IMU-driven attitude.
"""
import argparse
from datetime import datetime, timezone
import json
import time
from pathlib import Path
from cli import Link, require

def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--seconds', type=int, default=60, choices=range(5, 601))
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--check-rgb', action='store_true', help='require temporary SDK scanout probe')
    args = parser.parse_args()
    result = {'passed': False, 'target': args.port, 'seconds': args.seconds,
              'timestamp_utc': datetime.now(timezone.utc).isoformat()}
    link = Link(args.port)
    ok = lambda cmd: link.command(cmd, 'OK ')
    def wait(**expected):
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            line = ok('display status')
            values = fields(line)
            require(values['failed'] == 'false', line)
            if all(values[k] == v for k, v in expected.items()):
                return values
            time.sleep(.05)
        raise AssertionError(f'Display never reached {expected}: {line}')
    try:
        for cmd in ['source sim', 'display recover', 'display on', 'display gauge',
                    'fault pitch_deg off', 'fault roll_deg off', 'sim preset level',
                    'sim resume', 'instrument attitude']:
            ok(cmd)
        initial = wait(renderer='attitude', shown_quality='valid', backlight='true', shown_alt_ft='None')
        if args.check_rgb:
            require('rgb_short_eofs' in initial, 'RGB probe is not installed')
        for axis in ['pitch_deg', 'roll_deg']:
            ok(f'sim clear {axis}')
            wait(shown_quality='missing')
            ok(f'sim set {axis} 0')
            wait(shown_quality='valid', backlight='true')
            ok(f'fault {axis} on')
            wait(shown_quality='failed')
            ok(f'fault {axis} off')
            wait(shown_quality='valid', backlight='true')
        ok('sim pause')
        wait(shown_quality='stale')
        ok('sim resume')
        wait(shown_quality='valid', backlight='true')
        for mode in ['stall', 'transfer-stall']:
            ok('display ' + mode)
            start = time.monotonic()
            wait(backlight='false')
            result[mode + '_observed_ms'] = round((time.monotonic()-start)*1000, 1)
            require(result[mode + '_observed_ms'] < 600, 'Supervisor blanking exceeded polling budget')
            require('timing_fault=false' in ok('status'), 'Owner fault during display stall')
            ok('display recover')
            wait(shown_quality='valid', backlight='true')
        ok('instrument altimeter')
        wait(renderer='altimeter', shown_quality='valid', shown_alt_ft='Some(4520.0)')
        ok('instrument attitude')
        wait(renderer='attitude', shown_quality='valid', shown_alt_ft='None')
        ok('imu on')
        time.sleep(2)
        result['before'] = wait(backlight='true')
        ok(f'sim attitude {args.seconds}')
        ok('display stats reset')
        print(f'Attitude motion running for {args.seconds}s with IMU diagnostics on.', flush=True)
        # No diagnostic UART queries during the timed animation.
        deadline = time.monotonic() + args.seconds
        while time.monotonic() < deadline:
            time.sleep(min(1, max(0, deadline-time.monotonic())))
        result['stats'] = fields(ok('display stats stop'))
        result['after'] = wait(renderer='attitude', shown_quality='valid', backlight='true')
        # Let the final level sample pass through both confirmed transfers.
        time.sleep(.15)
        result['sample'] = ok('display sample')
        result['owner'] = ok('status')
        result['demo'] = ok('sim attitude status')
        result['imu'] = ok('imu status')
        require('state=complete' in result['demo'], 'Demo incomplete')
        require('timing_fault=false' in result['owner'], 'Owner fault')
        require('shown_pitch_deg=Some(0.0)' in result['sample'] and
                'shown_roll_deg=Some(0.0)' in result['sample'], 'Wrong final attitude')
        require(result['before']['blanked_at_ms'] == result['after']['blanked_at_ms'], 'Unexpected blanking during motion')
        require(result['before']['expired_ticks'] == result['after']['expired_ticks'], 'Numeric deadline missed during motion')
        require(float(result['stats']['rendered_fps']) >= 30, 'Below 30 completed FPS')
        if args.check_rgb:
            result['rgb_short_eof_delta'] = int(result['after']['rgb_short_eofs']) - int(initial['rgb_short_eofs'])
            require(result['rgb_short_eof_delta'] == 0, 'RGB refill deadline missed during attitude regression')
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        try:
            ok('display off')
        except Exception:
            pass
        raise
    finally:
        result['transcript'] = link.responses
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
        link.close()
    print(json.dumps({k: v for k, v in result.items() if k != 'transcript'}, indent=2))

if __name__ == '__main__':
    main()
