"""Human-observed replay of the repeated-altitude display corruption.

Adapted from the diagnosing-bugs HITL loop for this Windows/UART target.
Run to replay, then record the user's physical observation with --observe.
Counters alone cannot declare optical success. No reboot is requested.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import time
from cli import Link


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--observe', choices=['clean', 'artifact', 'uncertain'])
    parser.add_argument('--note', default='')
    parser.add_argument('--repeat', type=int, default=2)
    args = parser.parse_args()
    if args.observe:
        result = json.loads(args.report.read_text())
        result.update(optical_observation=args.observe, observation_note=args.note,
                      passed=(args.observe == 'clean' and result.get('software_checks_passed', False)))
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        print('Optical verdict:', args.observe)
        return
    if not 2 <= args.repeat <= 100:
        parser.error('--repeat must be 2..100')
    link = Link('COM5')
    result = dict(passed=False, optical_observation='pending',
                  timestamp_utc=datetime.now(timezone.utc).isoformat())
    try:
        result['before'] = link.command('display status', 'OK display')
        # Reproduce manual terminal traffic with board echo enabled.
        link.command('echo on', 'OK echo=on')
        link.command('sim clear pressure_altitude_ft', 'OK ')
        time.sleep(1)
        for index in range(args.repeat):
            link.command('sim set pressure_altitude_ft 8500', 'OK ')
            time.sleep(1)
            print(f'Issued 8500 ft setting {index + 1}/{args.repeat}', flush=True)
        result['status'] = link.command('status', 'OK status')
        result['display'] = link.command('display status', 'OK display')
        result['software_checks_passed'] = ('timing_fault=false' in result['status']
                                             and 'failed=false' in result['display'])
        print('Inspect the physical LCD: duplicated bottom arc, damaged left ticks, wrapping or shifts?')
        print(result['display'])
    finally:
        result['transcript'] = link.responses
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        link.close()


if __name__ == '__main__':
    main()
