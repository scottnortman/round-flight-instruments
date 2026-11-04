"""Exercise the real native connector/board with deliberately delayed bench data.

No simulator required. Close the tray app and other serial clients first.
Changes to local simulation for setup/cleanup; never enables aircraft writeback.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time
from cli import Link, require


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--config', required=True, type=Path, help='native enrollment for this same board')
    parser.add_argument('--connector', type=Path, default=Path('target/release/instrument-connector.exe'))
    parser.add_argument('--seconds', type=int, default=20, choices=range(5, 121))
    parser.add_argument('--sample-age-ms', type=int, default=35, choices=range(0, 1001))
    parser.add_argument('--gauges', nargs='+', choices=['altimeter', 'attitude', 'turn'], default=['altimeter', 'attitude', 'turn'])
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = {'passed': False, 'gauges': []}
    try:
        for gauge in args.gauges:
            link = Link(args.port)
            try:
                for command in ['source sim', 'sim preset level', 'touch cancel', 'instrument ' + gauge, 'display on']:
                    link.command(command, 'OK ')
                time.sleep(.5)
                before = fields(link.command('display status', 'OK '))
                link.command('display stats reset', 'OK ')
            finally:
                link.close()
            log = args.report.parent / (args.report.stem + '-' + gauge + '.log')
            with log.open('w', encoding='utf-8') as output:
                process = subprocess.run([str(args.connector.resolve()), 'run', '--config', str(args.config.resolve()),
                    '--synthetic', str(args.seconds + 2), '--synthetic-age-ms', str(args.sample_age_ms),
                    '--duration', str(args.seconds), '--sixpack', '--log-dir', str(args.report.parent / ('native-' + gauge))],
                    stdout=output, stderr=subprocess.STDOUT, timeout=args.seconds + 20)
            link = Link(args.port)
            try:
                stats = fields(link.command('display stats stop', 'OK '))
                after = fields(link.command('display status', 'OK '))
            finally:
                link.close()
            item = {'gauge': gauge, 'exit': process.returncode, 'before': before, 'after': after, 'stats': stats}
            report['gauges'].append(item)
            require(process.returncode == 0, 'Native connector failed; see ' + str(log))
            require(float(stats['rendered_fps']) >= 30 and after['failed'] == 'false', 'Rendering below the healthy 30 FPS requirement')
            require(int(stats['capture_age_max_ms']) >= args.sample_age_ms, 'No valid delayed samples reached this gauge')
            # Disconnected at clean exit is deliberate. Fresh-source expiry is
            # never allowed, even in startup/switching; total counters remain in
            # the report so intentional source-loss transitions are not hidden.
            require(before['expired_source_valid'] == after['expired_source_valid'], 'A numeric frame expired while the source was valid')
            print(f"PASS {gauge}: {stats['rendered_fps']} completed FPS, no valid-source expiry", flush=True)
        report['passed'] = True
    except Exception as error:
        report['error'] = repr(error)
        raise
    finally:
        args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
        link = Link(args.port)
        try:
            for command in ['source sim', 'sim preset level', 'baro 29.92', 'instrument altimeter']:
                link.command(command, 'OK ')
        finally:
            link.close()


if __name__ == '__main__':
    main()
