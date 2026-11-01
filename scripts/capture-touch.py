"""Read-only UART touch/overlay observation; no synthetic touches or setting changes."""
import argparse
import json
import time
from pathlib import Path
from cli import Link, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--seconds', type=int, default=30)
    parser.add_argument('--check-imu', action='store_true', help='also require an already running IMU to stay healthy')
    parser.add_argument('--check-sound', action='store_true', help='also verify buzzer readiness and error counters')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.seconds <= 180:
        parser.error('seconds must be 1..180')
    link = Link(args.port)
    result = {'passed': False, 'target': args.port, 'seconds': args.seconds,
              'imu_saturated_observations': 0}
    try:
        start = time.monotonic()
        last = None
        last_tick = None
        while time.monotonic() - start < args.seconds:
            touch = link.command('touch status', 'OK touch')
            state = link.command('status', 'OK status')
            display = link.command('display status', 'OK display')
            if args.check_sound:
                sound = link.command('touch sound status', 'OK touch sound')
                require('ready=true' in sound and 'errors=0' in sound, sound)
            if args.check_imu:
                imu = link.command('imu status')
                if imu != 'ERR imu_busy':
                    # Handling/rotating the board may saturate the sensor. That
                    # must remain explicitly flagged, but is not a bus failure.
                    saturated = 'quality=saturated' in imu
                    result['imu_saturated_observations'] += int(saturated)
                    require('state=running' in imu and
                            ('quality=valid' in imu or saturated) and 'errors=0' in imu, imu)
            tick = int(state.split('ticks=')[1].split()[0])
            require(last_tick is None or tick >= last_tick, 'board reset: ticks regressed')
            last_tick = tick
            key = (touch.split(' contact=')[0], state.split('baro_inhg=')[1].split()[0])
            if key != last:
                print(touch, flush=True)
                print(state, flush=True)
                last = key
            if 'timing_fault=false' not in state or 'failed=false' not in display:
                raise RuntimeError('owner/display failure during capture')
            time.sleep(.1)
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        result['transcript'] = link.responses
        result['check_imu'] = args.check_imu
        result['check_sound'] = args.check_sound
        result['device_lines'] = link.device_lines
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        link.close()


if __name__ == '__main__':
    main()
