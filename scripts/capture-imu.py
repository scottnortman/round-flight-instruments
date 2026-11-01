"""Record IMU diagnostics including rejected readings, with host receipt times."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import time
from cli import Link, require

def fields(line):
    return dict(item.split('=', 1) for item in line.split() if '=' in item)

def control(link, command):
    request = fields(link.command(command, 'OK imu requested='))['request']
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        status = link.command('imu status')
        data = fields(status)
        if data.get('completed') == request:
            require(data.get('state') != 'failed', status)
            return data
        time.sleep(0.05)
    raise TimeoutError(command + ' did not complete')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--seconds', type=float, default=15)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    require(1 <= args.seconds <= 120, 'seconds must be 1..120')
    link = Link(args.port)
    report = {'timestamp_utc': datetime.now(timezone.utc).isoformat(),
              'timebase': 'host receipt elapsed seconds; not sensor acquisition time',
              'events': [], 'completed': False}
    try:
        report['initial_status'] = link.command('status', 'OK status')
        control(link, 'imu off')
        report['identity'] = control(link, 'imu on')
        print(f'Motion capture running for {args.seconds:g} seconds. Gently tilt and rotate now.', flush=True)
        epoch = time.monotonic()
        while time.monotonic() - epoch < args.seconds:
            start = time.monotonic()
            line = link.command('imu read')
            end = time.monotonic()
            event = {'t_s': end-epoch, 'response_ms': (end-start)*1000, 'response': line}
            if line.startswith('OK imu sample='):
                event['sample'] = fields(line)
            else:
                event['error'] = line
            report['events'].append(event)
            time.sleep(0.05)
        report['final_sensor_status'] = link.command('imu status')
        report['final_owner_status'] = link.command('status')
        report['completed'] = True
        valid = sum('sample' in e for e in report['events'])
        print(f'Capture finished: {valid} valid readings; {len(report["events"])-valid} rejected readings.', flush=True)
    finally:
        try:
            report['shutdown'] = control(link, 'imu off')
        finally:
            report['transcript'] = link.responses
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
            link.close()

if __name__ == '__main__':
    main()
