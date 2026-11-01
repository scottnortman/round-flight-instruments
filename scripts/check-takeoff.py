"""Verify the bench climb on desktop or UART; leave the completed endpoint visible."""
import argparse
import json
from pathlib import Path
import time
from cli import Link, require


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port')
    parser.add_argument('--seconds', type=int, default=5)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    link = Link(args.port)
    result = {'passed': False, 'target': args.port or 'desktop'}
    ok = lambda text: link.command(text, 'OK ')
    try:
        ok('fault pressure_altitude_ft off')
        ok('sim takeoff 5')
        time.sleep(1.6)
        ok('sim takeoff stop')
        first = fields(ok('status'))['pressure_altitude_ft'].split(':')[0]
        time.sleep(.15)
        require(fields(ok('status'))['pressure_altitude_ft'].split(':')[0] == first, 'Stop drifted')
        ok('sim takeoff 5')
        ok('sim set pressure_altitude_ft 8500')
        time.sleep(.1)
        require('state=stopped' in ok('sim takeoff status'), 'Manual input did not stop sweep')
        require('8500.000:valid:' in ok('status'), 'Manual altitude ignored')
        ok('fault pressure_altitude_ft on')
        require(link.command('sim takeoff').startswith('ERR not_ready'), 'Fault bypassed')
        ok('fault pressure_altitude_ft off')
        require(link.command('sim takeoff 0') == 'ERR range', 'Invalid duration accepted')
        ok(f'sim takeoff {args.seconds}')
        print(f'Sweep running: 1 second at zero, then {args.seconds} seconds to 10000 ft.', flush=True)
        start = time.monotonic()
        altitudes = []
        while time.monotonic() - start < args.seconds + 6:
            state = fields(ok('status'))
            require(state['timing_fault'] == 'false', str(state))
            altitude, quality, _ = state['pressure_altitude_ft'].split(':')
            if quality == 'valid':
                altitude = float(altitude)
                require(0 <= altitude <= 10000, str(altitude))
                require(not altitudes or altitude >= altitudes[-1], 'Non-monotonic climb')
                altitudes.append(altitude)
            sweep = fields(ok('sim takeoff status'))
            if sweep['state'] == 'complete':
                break
            time.sleep(.2)
        require(sweep['state'] == 'complete', 'Sweep did not complete')
        require(altitudes and altitudes[0] == 0 and altitudes[-1] == 10000, str(altitudes))
        result['samples'] = len(altitudes)
        if args.port:
            stats = fields(ok('display stats'))
            require(stats['state'] == 'stopped', str(stats))
            require(int(stats['frames']) > 0 and float(stats['rendered_fps']) > 0, str(stats))
            require(stats['failed'] == 'false', str(stats))
            time.sleep(.4)
            later = fields(ok('display stats'))
            require(all(later[k] == stats[k] for k in ['frames', 'elapsed_ms', 'rendered_fps']), 'Final statistics drifted')
            display = ok('display status')
            require('shown_alt_ft=Some(10000.0)' in display and 'backlight=true' in display, display)
            result['stats'] = stats
            result['display'] = display
        result['final_status'] = ok('status')
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        try: ok('sim takeoff stop')
        except Exception: pass
        raise
    finally:
        result['transcript'] = link.responses
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        link.close()
    print(json.dumps({k: v for k, v in result.items() if k != 'transcript'}, indent=2))


if __name__ == '__main__':
    main()
