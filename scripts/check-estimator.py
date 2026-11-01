"""Check estimator hooks and read-format diagnostics on the board; finish IMU off.

No movement is required. This does not certify stationarity or calibrate the gyro.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import statistics
import time
from cli import Link, require


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def command(link, text):
    for _ in range(20):
        line = link.command(text)
        if line != 'ERR imu_busy':
            require(line.startswith('OK '), line)
            return line
        time.sleep(0.05)
    raise TimeoutError('IMU snapshot busy')


def control(link, operation, state):
    ack = fields(command(link, 'imu ' + operation))
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        line = command(link, 'imu status')
        data = fields(line)
        if data['completed'] == ack['request']:
            require(data['state'] == state, line)
            return line
        time.sleep(0.05)
    raise TimeoutError('IMU request did not complete')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--report', required=True, type=Path)
    args = parser.parse_args()
    link = Link(args.port)
    result = {'passed': False, 'timestamp_utc': datetime.now(timezone.utc).isoformat()}
    try:
        before = fields(command(link, 'status'))
        display_before = (fields(command(link, 'display status'))
                          if before.get('display') == 'managed' else None)
        control(link, 'off', 'off')
        off = command(link, 'imu estimator')
        require('quality=unavailable' in off and 'roll_deg=none' in off, off)
        control(link, 'on', 'running')
        audits = []
        for _ in range(3):
            line = control(link, 'audit', 'running')
            require('config_match:true;data_match:true' in line, line)
            audits.append(line)
        result['audits'] = audits
        samples = []
        estimates = []
        for _ in range(20):
            raw = fields(command(link, 'imu read'))
            est = command(link, 'imu estimator')
            data = fields(est)
            require(data['estimator'] == 'accel_tilt', est)
            require(data['quality'] in ('bench_only', 'unavailable'), est)
            require(data['yaw_deg'] == 'None' and data['attitude_covariance'] == 'unknown', est)
            require(data['gyro_bias_rad_s'] == 'None' and data['gauge_eligible'] == 'false', est)
            samples.append(raw)
            estimates.append(est)
            time.sleep(0.1)
        require(any('quality=bench_only' in s for s in estimates), 'No usable bench tilt observed')
        result['estimates'] = estimates
        result['raw_gyro_mean_dps'] = [statistics.mean(float(s[k]) for s in samples)
                                       for k in ('gx_dps', 'gy_dps', 'gz_dps')]
        result['samples'] = samples
        control(link, 'off', 'off')
        off = command(link, 'imu estimator')
        require('quality=unavailable' in off and 'roll_deg=none' in off, off)
        # Re-enable confirms the estimator restarts cleanly across source epochs.
        control(link, 'on', 'running')
        time.sleep(0.1)
        require('quality=bench_only' in command(link, 'imu estimator'), 'Estimator did not restart')
        after = fields(command(link, 'status'))
        for key in ('source', 'instrument', 'baro_inhg'):
            require(before[key] == after[key], f'Changed {key}')
        require(after['timing_fault'] == 'false', 'Owner timing fault')
        if display_before is not None:
            display_after = fields(command(link, 'display status'))
            require(display_after['failed'] == 'false', str(display_after))
            require(display_after['backlight'] == 'true', str(display_after))
            require(int(display_after['frames']) > int(display_before['frames']),
                    'Display stopped progressing during IMU checks')
            require(display_after['blanked_at_ms'] == display_before['blanked_at_ms'],
                    'Display guard blanked during IMU checks')
            result['display_before'] = display_before
            result['display_after'] = display_after
        result['final_status'] = after
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        try:
            result['shutdown'] = control(link, 'off', 'off')
        except Exception as error:
            result['passed'] = False
            result['shutdown_error'] = repr(error)
        result['transcript'] = link.responses
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        link.close()
    require(result['passed'], 'Shutdown failed; inspect report')
    print(json.dumps({k: v for k, v in result.items()
                      if k not in ('transcript', 'samples', 'estimates')}, indent=2))


if __name__ == '__main__':
    main()
