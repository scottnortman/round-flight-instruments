"""Board memory/performance regression for each implemented gauge.

Uses explicit local simulation, then restores the level altimeter. Requires
pyserial. No UART diagnostics run during timed motion; cumulative guard counters
detect even brief blanking. Reports overlapping heaps separately.
"""
import argparse
import json
from pathlib import Path
import time
from cli import Link, require


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--seconds', type=int, default=30, choices=range(5, 301))
    parser.add_argument('--report', required=True, type=Path)
    args = parser.parse_args()
    link = Link(args.port)
    result = {'passed': False, 'gauges': []}
    ok = lambda command: link.command(command, 'OK ')
    try:
        for command in ['source sim', 'imu off', 'sim preset level', 'sim resume',
                        'touch cancel', 'display recover', 'display gauge', 'display on']:
            ok(command)
        for gauge, demo in [('altimeter', 'takeoff'), ('attitude', 'attitude'), ('turn', 'turn'), ('airspeed', 'airspeed'), ('heading', 'heading'), ('vsi', 'vsi')]:
            ok(f'sim {demo} {args.seconds + 2}')
            time.sleep(1)  # Separate view entry from the steady-motion window.
            before_display = fields(ok('display status'))
            before = fields(ok('memory status'))
            require(before['app_offset_bytes'] == '65536' and before['app_size_bytes'] == '4194304',
                    'Installed application partition differs from the 4 MiB layout')
            ok('display stats reset')
            time.sleep(args.seconds)
            stats = fields(ok('display stats stop'))
            after_display = fields(ok('display status'))
            after = fields(ok('memory status'))
            item = {'gauge': gauge, 'before': before, 'after': after, 'stats': stats,
                    'before_display': before_display, 'after_display': after_display}
            result['gauges'].append(item)
            require(float(stats['rendered_fps']) >= 30 and stats['failed'] == 'false', str(stats))
            require(int(after['display_stack_min_free_bytes']) >= 2048,
                    f'{gauge}: less than 2 KiB renderer stack headroom')
            require(after_display['renderer'] == gauge and after_display['shown_quality'] == 'valid', str(after_display))
            for counter in ['blanked_at_ms', 'expired_ticks']:
                require(before_display[counter] == after_display[counter], f'{gauge}: changed {counter}')
            for kind in ['internal', 'psram']:
                require(before[kind + '_free_bytes'] == after[kind + '_free_bytes'], f'{gauge}: heap changed')
                require(int(after[kind + '_largest_bytes']) <= int(after[kind + '_free_bytes']) <= int(after[kind + '_total_bytes']),
                        f'{gauge}: invalid {kind} accounting')
            print(f"PASS {gauge}: {stats['rendered_fps']} FPS, PSRAM free {after['psram_free_bytes']} B", flush=True)
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        try:
            for command in ['source sim', 'sim preset level', 'instrument altimeter']:
                ok(command)
            result['final_memory'] = fields(ok('memory status'))
        finally:
            result['transcript'] = link.responses
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
            link.close()


if __name__ == '__main__':
    main()
