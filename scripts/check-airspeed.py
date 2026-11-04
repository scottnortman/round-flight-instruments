"""Real UART IAS mapping, independent TAS, invalid data and motion regression.

No simulator needed. Uses explicit local simulation and synthetic T2 packets;
never enables aircraft writeback. Leaves local IAS 85 KIAS selected, COM released.
"""
import argparse
import json
from pathlib import Path
import time
import zlib
from cli import Link, require


def fields(line):
    return dict(w.split('=', 1) for w in line.split() if '=' in w)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--port', default='COM5')
    p.add_argument('--seconds', type=int, default=10, choices=range(5, 121))
    p.add_argument('--report', type=Path, required=True)
    args = p.parse_args()
    result = {'passed': False}
    link = Link(args.port)
    ok = lambda c: link.command(c, 'OK ')

    def wait_sample(quality='valid', ias=None, over=False):
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            line = link.command('display sample')
            if line == 'ERR display_sample_busy':
                continue
            require(line.startswith('OK display_sample '), line)
            data = fields(line)
            expected = 'None' if ias is None else f'Some({float(ias)})'
            if (data['renderer'] == 'airspeed' and data['quality'] == quality
                    and data['shown_ias_kt'] == expected
                    and data['ias_over_range'] == str(over).lower()):
                return data
            time.sleep(.02)
        raise AssertionError('IAS presentation did not reach expected state: ' + line)

    try:
        for c in ['source sim', 'imu off', 'touch cancel', 'sim preset level',
                  'instrument airspeed', 'display recover', 'display gauge', 'display on']:
            ok(c)
        require('airspeed' in fields(ok('protocol capabilities'))['gauges'], 'IAS capability not advertised')
        result['local_values'] = []
        for value in [0, 20, 40, 85, 120, 163, 200]:
            ok(f'sim set ias_kt {value}')
            result['local_values'].append(wait_sample(ias=value))
        for value in ['-1', '201', 'NaN']:
            require(link.command('sim set ias_kt ' + value).startswith('ERR '), 'Invalid local input accepted')
        wait_sample(ias=200)
        ok('sim clear ias_kt'); wait_sample('missing')
        ok('sim set ias_kt 120'); wait_sample(ias=120)
        ok('fault ias_kt on'); wait_sample('failed')
        ok('fault ias_kt off'); wait_sample(ias=120)
        ok('sim pause'); wait_sample('stale')
        ok('sim resume'); wait_sample(ias=120)
        before = fields(ok('display status'))
        ok(f'sim airspeed {args.seconds}')
        ok('display stats reset')
        time.sleep(args.seconds + .1)
        result['motion_stats'] = fields(ok('display stats stop'))
        after = fields(ok('display status'))
        wait_sample(ias=85)
        require(float(result['motion_stats']['rendered_fps']) >= 30, 'Below 30 completed FPS')
        for key in ['expired_ticks', 'blanked_at_ms']:
            require(before[key] == after[key], 'Unexpected motion expiry/blanking')

        session = time.time_ns() // 1000
        ok(f'source external2 synthetic {session}')
        sequence = 0

        def frame(values):
            nonlocal sequence
            sequence += 1
            now = fields(ok('source status'))['now_ms']
            body = f'T2 {session} {sequence} {now} running {values}'
            ok(body + f' *{zlib.crc32(body.encode()):08X}')

        frame('6:0:0:120 12:0:0:300')
        result['ias_not_tas'] = wait_sample(ias=120)
        frame('12:0:0:300')
        result['missing_ias_no_tas_fallback'] = wait_sample('missing')
        frame('6:0:0:350 12:0:0:100')
        result['over_range'] = wait_sample(ias=350, over=True)
        frame('6:0:0:! 12:0:0:100'); wait_sample('failed')
        frame('6:0:100:85 12:0:0:300')
        aged = wait_sample(ias=85)
        require(int(aged['source_age_at_present_ms'].removeprefix('Some(').removesuffix(')')) >= 100,
                'Source age was discarded')
        result['external_stale'] = wait_sample('stale')
        result['owner'] = ok('status')
        require('timing_fault=false' in result['owner'], 'Owner timing fault')
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        try:
            for c in ['source sim', 'touch cancel', 'sim preset level', 'instrument airspeed',
                      'sim set ias_kt 85', 'display recover', 'display on']:
                ok(c)
            result['final_memory'] = ok('memory status')
        finally:
            result['transcript'] = link.responses
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(result, indent=2) + '\n')
            link.close()
    print('PASS airspeed: local range/invalidity, 30+ FPS, T2 IAS/TAS isolation, over-range and stale handling.')


if __name__ == '__main__':
    main()
