"""Bench regression: switch local simulated gauges without expiry or blanking.

Changes the source to simulation and leaves the altimeter at 4520 ft / 29.92.
Does not synthesize touches or relax any firmware freshness checks.
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
    parser.add_argument('--count', type=int, default=12, choices=range(2, 51, 2))
    parser.add_argument('--gauges', nargs='+', choices=['altimeter', 'attitude', 'turn'],
                        default=['attitude', 'altimeter'])
    parser.add_argument('--check-rgb', action='store_true', help='require temporary SDK scanout probe')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    link = Link(args.port)
    result = {'passed': False, 'switches': args.count, 'observations': []}
    ok = lambda command: link.command(command, 'OK ')
    try:
        for command in ['source sim', 'imu off', 'sim preset level', 'sim resume',
                        'baro 29.92', 'touch cancel', 'instrument altimeter',
                        'display gauge', 'display recover', 'display on']:
            ok(command)
        time.sleep(1)
        before = fields(ok('display status'))
        result['before'] = before
        if args.check_rgb:
            require('rgb_short_eofs' in before, 'RGB probe is not installed')
        started = time.monotonic()
        for index in range(args.count):
            gauge = args.gauges[index % len(args.gauges)]
            ok('instrument ' + gauge)
            deadline = time.monotonic() + 2
            while True:
                display = fields(ok('display status'))
                result['observations'].append(display)
                if args.check_rgb:
                    require(display.get('rgb_short_eofs') == before['rgb_short_eofs'],
                            'RGB refill deadline missed; DMA recovery occurred')
                require(display['failed'] == 'false', 'Display worker failed')
                require(display['blanked_at_ms'] == before['blanked_at_ms'],
                        'Unexpected blanking while switching gauges')
                require(display['expired_ticks'] == before['expired_ticks'],
                        'Numeric presentation expired during switch')
                if display['renderer'] == gauge and display['shown_quality'] == 'valid':
                    break
                require(time.monotonic() < deadline, 'Gauge did not become visible')
                time.sleep(.03)
            time.sleep(.3)
        after = fields(ok('display status'))
        result['after'] = after
        if args.check_rgb:
            require(after.get('rgb_short_eofs') == before['rgb_short_eofs'],
                    'RGB refill deadline missed at final switch')
        require(after['blanked_at_ms'] == before['blanked_at_ms'], 'Blanking at final switch')
        require(after['expired_ticks'] == before['expired_ticks'], 'Expiry at final switch')
        result['presented_fps'] = round((int(after['frames']) - int(before['frames'])) /
                                        (time.monotonic() - started), 2)
        require(result['presented_fps'] >= 30, 'Switching presentation rate below 30 Hz')
        require('timing_fault=false' in ok('status'), 'Owner timing fault')
        result['passed'] = True
        print(json.dumps({k: v for k, v in result.items() if k != 'observations'}, indent=2))
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        result['transcript'] = link.responses
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        link.close()


if __name__ == '__main__':
    main()
