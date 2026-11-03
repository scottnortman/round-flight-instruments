"""Detect even brief supervisor blanking during a steady simulated attitude face.

Checks cumulative counters, so a blackout between polls cannot pass unnoticed.
Does not inject touch events, stop the simulator clock, or change safety limits.
"""
import argparse
import json
import time
from pathlib import Path
from cli import Link, require


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--seconds', type=int, default=180, choices=range(5, 601))
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--check-rgb', action='store_true', help='require temporary SDK scanout probe')
    args = parser.parse_args()
    link = Link(args.port)
    result = {'passed': False, 'observations': []}
    ok = lambda command: link.command(command, 'OK ')
    try:
        for command in ['source sim', 'imu off', 'sim preset level', 'sim resume',
                        'touch cancel', 'instrument attitude', 'display gauge',
                        'display recover', 'display on']:
            ok(command)
        time.sleep(1)
        before = fields(ok('display status'))
        result['before'] = before
        if args.check_rgb:
            require('rgb_short_eofs' in before, 'RGB probe is not installed')
        require(before['backlight'] == 'true' and before['shown_quality'] == 'valid',
                'Steady attitude did not initialize')
        started = time.monotonic()
        while time.monotonic() - started < args.seconds:
            time.sleep(.5)
            display = fields(ok('display status'))
            result['observations'].append(display)
            if args.check_rgb:
                require(display.get('rgb_short_eofs') == before['rgb_short_eofs'],
                        'RGB refill deadline missed while idle; DMA recovery occurred')
            require(display['failed'] == 'false' and display['backlight'] == 'true',
                    'Display failed or blanked while idle')
            require(display['blanked_at_ms'] == before['blanked_at_ms'],
                    'Unexpected brief idle blanking')
            require(display['expired_ticks'] == before['expired_ticks'],
                    'Idle numeric presentation expired')
            require(display['shown_quality'] == 'valid', 'Idle attitude lost valid data')
        result['passed'] = True
        result['elapsed_s'] = round(time.monotonic() - started, 3)
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        result['transcript'] = link.responses
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps({k: v for k, v in result.items()
                          if k not in ('transcript', 'observations')}, indent=2), flush=True)
        link.close()


if __name__ == '__main__':
    main()
