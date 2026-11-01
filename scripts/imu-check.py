"""Exercise real IMU diagnostics, capture raw data, and report bench statistics.

This does not calibrate the device or select it as a gauge source.
--motion additionally requires observed movement during capture.
"""
import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import statistics
import time
from cli import Link, require


def values(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def wait_state(link, target, timeout=4):
    expected = values(link.responses[-1]['response']).get('request')
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        response = link.command('imu status')
        if response == 'ERR imu_busy':
            time.sleep(0.05)
            continue
        require(response.startswith('OK imu'), response)
        data = values(response)
        if expected is not None and data.get('completed') != expected:
            time.sleep(0.05)
            continue
        require(data['state'] != 'failed', response)
        if data['state'] == target:
            return data
        time.sleep(0.05)
    raise TimeoutError(f'IMU did not enter {target}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--seconds', type=float, default=10)
    parser.add_argument('--motion', action='store_true')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    require(1 <= args.seconds <= 120, 'seconds must be 1..120')
    link = Link(args.port)
    samples = []
    result = {'passed': False, 'timestamp_utc': datetime.now(timezone.utc).isoformat(),
              'motion_required': args.motion, 'verification_scope': 'driver_transport_and_control'}
    try:
        before = values(link.command('status', 'OK status'))
        link.command('imu off', 'OK imu requested=off')
        wait_state(link, 'off')
        link.command('imu read', 'ERR imu_off')
        link.command('imu on extra', 'ERR syntax')
        require(values(link.command('imu status', 'OK imu'))['state'] == 'off', 'malformed command enabled IMU')
        link.command('imu on', 'OK imu requested=on')
        start = wait_state(link, 'running')
        require(start['who'] == '0x05' and start['detected'] == 'true', 'wrong sensor identity')
        print(f"IMU capture running for {args.seconds:g}s; " +
              ('gently tilt/rotate the board now.' if args.motion else 'recording current sensor readings.'), flush=True)
        deadline = time.monotonic() + args.seconds
        while time.monotonic() < deadline:
            line = link.command('imu read')
            if line in ('ERR imu_busy', 'ERR imu_missing'):
                continue
            require(line.startswith('OK imu sample='), line)
            row = values(line)
            for key in ('ax_g','ay_g','az_g','gx_dps','gy_dps','gz_dps','temp_c','norm_g'):
                row[key] = float(row[key])
                require(math.isfinite(row[key]), f'nonfinite {key}')
            row['counter'] = int(row['counter'])
            row['sample'] = int(row['sample'])
            require(int(row['age_ms']) < 100, 'stale sample published as valid')
            if samples:
                require(row['sample'] > samples[-1]['sample'], 'host received duplicate sample')
                delta = (row['counter'] - samples[-1]['counter']) & 0xffffff
                require(0 < delta < 0x800000, 'sensor counter did not progress')
            samples.append(row)
            time.sleep(0.05)
        stop = values(link.command('imu status', 'OK imu'))
        require(stop['quality'] == 'valid' and int(stop['errors']) == 0, 'unhealthy IMU after capture')
        require(len(samples) >= 5, 'too few samples')
        spans = {axis: max(r[axis] for r in samples) - min(r[axis] for r in samples)
                 for axis in ('ax_g','ay_g','az_g')}
        peak_gyro = max(abs(r[k]) for r in samples for k in ('gx_dps','gy_dps','gz_dps'))
        moved = max(spans.values()) > 0.20 and peak_gyro > 5
        if args.motion:
            require(moved, 'movement not observed; repeat while gently tilting/rotating')
        means = {key: statistics.mean(r[key] for r in samples)
                 for key in ('ax_g','ay_g','az_g','gx_dps','gy_dps','gz_dps','norm_g','temp_c')}
        result['stationary_gyro_offset_check'] = ('outside_10_dps_if_stationary' if
            max(abs(means[k]) for k in ('gx_dps','gy_dps','gz_dps')) > 10 else
            'within_10_dps_if_stationary')
        after = values(link.command('status', 'OK status'))
        require(after['timing_fault'] == 'false', 'sensor work delayed the state owner')
        for key in ('source','instrument','baro_inhg','sim'):
            require(after[key] == before[key], f'IMU diagnostics changed {key}')
        link.command('imu off', 'OK imu requested=off')
        off = wait_state(link, 'off')
        link.command('imu read', 'ERR imu_off')
        # Restart confirms reset/configuration and counters recover from disable.
        link.command('imu on', 'OK imu requested=on')
        restarted = wait_state(link, 'running')
        require(restarted['who'] == '0x05', 're-enable failed')
        result.update(passed=True, identity=start, final_capture=stop, off=off,
                      samples_captured=len(samples), observed_motion=moved,
                      acceleration_span_g=spans, peak_abs_gyro_dps=peak_gyro,
                      mean=means, max_owner_gap_ms=int(after['max_gap_ms']))
        print(json.dumps({k:v for k,v in result.items() if k not in ('identity','final_capture','off')}, indent=2))
    finally:
        # Do not leave a physical diagnostic source enabled after tests.
        try:
            link.command('imu off', 'OK imu requested=off')
            wait_state(link, 'off')
        finally:
            result['samples'] = samples
            result['transcript'] = link.responses
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
            link.close()


if __name__ == '__main__':
    main()
