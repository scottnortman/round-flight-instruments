"""Check real touch diagnostics/CLI bounds; no synthetic touch or barometer edits."""
import argparse
import json
import time
from pathlib import Path
from cli import Link, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    link = Link(args.port)
    result = {'passed': False, 'target': args.port}
    try:
        before = link.command('status', 'OK status')
        baro = before.split('baro_inhg=')[1].split()[0]
        initial = link.command('touch status', 'OK touch')
        original_hold = initial.split('hold_ms=')[1].split()[0]
        require('ready=true' in initial, initial)
        require('touch status|cancel|hold MS' in link.command('help', 'OK help'), 'touch help missing')
        link.command('touch cancel', 'OK touch edit=cancelled')
        link.command('touch hold 1500', 'OK touch hold_ms=1500')
        for command in ('touch hold 0', 'touch hold 249', 'touch hold 5001',
                        'touch hold NaN', 'touch hold 2000 extra', 'touch press 338 235'):
            require(link.command(command).startswith('ERR '), command)
        require('hold_ms=1500' in link.command('touch status'), 'rejected command changed hold')
        link.command('touch hold ' + original_hold, 'OK touch')
        time.sleep(.2)
        after = link.command('touch status', 'OK touch')
        require(int(after.split('polls=')[1].split()[0]) > int(initial.split('polls=')[1].split()[0]), 'touch polling stalled')
        require('errors=0' in after and 'last_error=None' in after, after)
        final = link.command('status', 'OK status')
        require('baro_inhg=' + baro in final and 'timing_fault=false' in final, final)
        result.update(passed=True, touch=after, final_status=final)
    finally:
        result['transcript'] = link.responses
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        link.close()
    print(json.dumps({k: v for k, v in result.items() if k != 'transcript'}, indent=2))


if __name__ == '__main__':
    main()
