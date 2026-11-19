"""Explicit board showcase regression; uses local/synthetic inputs, never MSFS.

Changes source/selection and temporarily exercises IMU acquisition. Success
leaves Combined in DEMO for physical review; failure returns to local Altimeter.
Physical touch and handheld tilt still need human acceptance.
"""
import argparse
import json
import time
import zlib
from pathlib import Path
from cli import Link, require


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--seconds', type=int, default=5, choices=range(3, 61))
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    link = Link(args.port)
    report = {'passed': False, 'gauges': []}
    def ok(command):
        # Snapshot reads deliberately use try_lock on the device. A bounded
        # retry handles that documented transient without relaxing assertions.
        for attempt in range(6):
            response = link.command(command)
            transient = ((response == 'ERR imu_busy' and command.startswith('imu ')) or
                         (response == 'ERR display_sample_busy' and command == 'display sample'))
            if transient and attempt < 5:
                time.sleep(.05)
                continue
            require(response.startswith('OK '), repr((command, response)))
            return response
    try:
        report['identity_before'] = ok('device info')
        require(fields(ok('touch status'))['menu_hold_ms'] == '1000', 'Menu default is not one second')
        for command in ['demo off', 'source sim', 'sim preset level', 'imu off',
                        'touch cancel', 'display on', 'display gauge']:
            ok(command)
        for gauge in ['altimeter', 'airspeed', 'heading', 'vsi', 'attitude', 'turn', 'consolidated', 'debug']:
            ok('instrument ' + gauge)
            ok('demo on')
            time.sleep(1)
            before = fields(ok('display status'))
            demo = fields(ok('demo status'))
            imu = fields(ok('imu status'))
            require(demo['active'] == 'true' and demo['screen'] == gauge, str(demo))
            require(ok('control poll').startswith('OK control none'), 'Demo queued an aircraft write')
            require(link.command('baro 30.01').startswith('ERR '), 'Demo accepted a barometer edit')
            ok('display stats reset')
            time.sleep(args.seconds)
            stats = fields(ok('display stats stop'))
            after = fields(ok('display status'))
            sample = ok('display sample')
            require(float(stats['rendered_fps']) >= 30 and stats['failed'] == 'false', str(stats))
            require(after['renderer'] == gauge and after['shown_quality'] == 'valid', str(after))
            require(before['deadline_blanks'] == after['deadline_blanks'], 'Demo blanked the display')
            ok('demo off')
            time.sleep(.65)
            imu_after = fields(ok('imu status'))
            require(imu_after['state'] == 'off', 'Demo did not restore previously-off IMU')
            report['gauges'].append({'gauge': gauge, 'demo': demo, 'imu': imu,
                                    'imu_after': imu_after, 'before': before,
                                    'after': after, 'sample': sample, 'stats': stats})
            print(f"PASS {gauge}: {stats['rendered_fps']} FPS, {demo['input']}", flush=True)

        # One DEMO session survives every screen and keeps its IMU acquisition.
        ok('instrument attitude')
        ok('demo on')
        time.sleep(.7)
        imu_before = fields(ok('imu status'))
        report['demo_switches'] = []
        for gauge in ['heading', 'altimeter', 'airspeed', 'vsi', 'turn', 'attitude', 'consolidated', 'debug']:
            ok('instrument ' + gauge)
            time.sleep(.65 if gauge == 'heading' else .15)
            demo = fields(ok('demo status'))
            imu = fields(ok('imu status'))
            require(demo['active'] == 'true' and demo['screen'] == gauge, 'Screen change ended DEMO')
            require(imu['state'] == 'running' and imu['request'] == imu_before['request'],
                    'Screen change restarted or stopped the DEMO sensor')
            report['demo_switches'].append({'gauge': gauge, 'demo': demo, 'imu': imu})
        ok('demo off')
        time.sleep(.65)
        require(fields(ok('imu status'))['state'] == 'off', 'DEMO exit did not release owned IMU')
        print('PASS retained DEMO and IMU through all eight screens', flush=True)

        # A sensor already running before DEMO belongs to the user, not DEMO.
        ok('imu on')
        time.sleep(.7)
        ok('instrument attitude')
        ok('demo on')
        time.sleep(.3)
        ok('demo off')
        time.sleep(.65)
        report['preexisting_imu'] = fields(ok('imu status'))
        require(report['preexisting_imu']['state'] == 'running', 'Demo stopped preexisting sensor acquisition')
        ok('imu off')

        # Continue receiving a real protocol stream behind the display overlay.
        import ctypes
        require(ctypes.windll.winmm.timeBeginPeriod(1) == 0, 'Cannot request timing resolution')
        try:
            session = time.time_ns() // 1000
            at = int(fields(ok(f'source external2 synthetic {session}'))['now_ms'])
            sequence = 0

            def feed():
                nonlocal sequence, at
                sequence += 1
                values = [8123., 8000., 30.12, 5., -15., 142., 220., 750., -2., .3]
                body = f'T2 {session} {sequence} {at} running' + ''.join(
                    f' {index}:0:1:{value:.5f}' for index, value in enumerate(values, 1))
                response = ok(f'{body} *{zlib.crc32(body.encode()):08X}')
                at = int(fields(response)['now_ms'])

            feed()
            ok('instrument debug')
            feed()
            ok('demo on')
            deadline = time.perf_counter() + 1.5
            while time.perf_counter() < deadline:
                feed()
            demo_source = ok('source status')
            feed()
            demo_sample = ok('display sample')
            feed()
            require(f'session={session}' in demo_source and 'indicated_ft=Some(8123.0)' in demo_source,
                    'Underlying source changed or stopped while DEMO was visible')
            require('shown_alt_ft=Some(8123.0)' not in demo_sample, 'Demo used live altitude')
            ok('demo off')
            deadline = time.perf_counter() + .4
            while time.perf_counter() < deadline:
                feed()
            restored = ok('display sample')
            feed()
            source_after = ok('source status')
            require('shown_alt_ft=Some(8123.0)' in restored, 'Demo exit did not restore latest live reading')
            require(f'session={session}' in source_after, 'Demo exit replaced the live session')
            for field, value in [('alt_ft', 8123.0), ('pressure_alt_ft', 8000.0), ('ias_kt', 142.0), ('pitch_deg', 5.0),
                                 ('roll_deg', -15.0), ('heading_deg', 220.0), ('vsi_fpm', 750.0),
                                 ('turn_dps', -2.0), ('ball', 0.3), ('baro_inhg', 30.12)]:
                require(f'shown_{field}=Some({value})' in restored,
                        'DEBUG did not present source value: ' + field + ': ' + restored)
            report['stream'] = {'session': session, 'frames': sequence, 'during': demo_source,
                                'demo_sample': demo_sample, 'restored': restored, 'after': source_after}
        finally:
            ctypes.windll.winmm.timeEndPeriod(1)

        for command in ['source sim', 'sim preset level', 'instrument consolidated', 'demo on']:
            ok(command)
        time.sleep(.7)
        report['final'] = {command: ok(command) for command in [
            'device info', 'settings status', 'demo status', 'status', 'imu status',
            'memory status', 'display status', 'display sample']}
        require(report['identity_before'].split('uuid=')[1].split()[0] ==
                report['final']['device info'].split('uuid=')[1].split()[0], 'Device UUID changed')
        require(int(fields(report['final']['memory status'])['display_stack_min_free_bytes']) >= 2048,
                'Renderer stack headroom is below 2 KiB')
        report['passed'] = True
    finally:
        if not report['passed']:
            for command in ['demo off', 'source sim', 'sim preset level', 'instrument altimeter']:
                ok(command)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
        link.close()


if __name__ == '__main__':
    main()
