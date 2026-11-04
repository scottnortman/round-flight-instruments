"""Real UART Heading/VSI semantics and failure isolation; no MSFS or writes.

Uses local simulation and CRC-protected T2 test data. Restores level simulation,
leaves all six gauges selectable, and releases COM on success or failure.
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
    p.add_argument('--report', type=Path, required=True)
    args = p.parse_args()
    result = {'passed': False, 'observations': []}
    link = Link(args.port)
    ok = lambda c: link.command(c, 'OK ')

    def presented(gauge, quality='valid', value=None):
        key = 'shown_heading_deg' if gauge == 'heading' else 'shown_vsi_fpm'
        expected = 'None' if value is None else f'Some({float(value)})'
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            line = link.command('display sample')
            if line == 'ERR display_sample_busy':
                continue
            require(line.startswith('OK display_sample '), line)
            sample = fields(line)
            if sample['renderer'] == gauge and sample['quality'] == quality and sample[key] == expected:
                result['observations'].append(sample)
                return sample
            time.sleep(.01)
        raise AssertionError(f'{gauge} expected {quality}/{expected}, got {line}')

    try:
        for c in ['source sim', 'imu off', 'touch cancel', 'sim preset level',
                  'display recover', 'display gauge', 'display on']:
            ok(c)
        capability = fields(ok('protocol capabilities'))['gauges']
        require(all(g in capability for g in ['airspeed', 'attitude', 'altimeter', 'heading', 'turn', 'vsi']), capability)
        for gauge, field, values, bad in [
            ('heading', 'heading_deg', [0, 90, 180, 270, 359], ['-1', '360', 'NaN']),
            ('vsi', 'vsi_fpm', [0, 1000, -1000, 2000, -2000, 3500, -3500], ['-6001', '6001', 'NaN']),
        ]:
            ok('instrument ' + gauge)
            for value in values:
                ok(f'sim set {field} {value}'); presented(gauge, value=value)
            for value in bad:
                require(link.command(f'sim set {field} {value}').startswith('ERR '), 'Invalid local input accepted')
            ok(f'sim clear {field}'); presented(gauge, 'missing')
            ok(f'sim set {field} 0'); presented(gauge, value=0)
            ok(f'fault {field} on'); presented(gauge, 'failed')
            ok(f'fault {field} off'); presented(gauge, value=0)
            ok('sim pause'); presented(gauge, 'stale')
            ok('sim resume'); presented(gauge, value=0)

        session = time.time_ns() // 1000
        ok(f'source external2 synthetic {session}')
        sequence = 0

        def frame(values):
            nonlocal sequence
            sequence += 1
            now = fields(ok('source status'))['now_ms']
            body = f'T2 {session} {sequence} {now} running {values}'
            ok(body + f' *{zlib.crc32(body.encode()):08X}')

        ok('instrument heading')
        frame('7:0:0:90 11:0:0:180 8:0:0:1000')
        presented('heading', value=90)  # Magnetic, never true heading.
        frame('11:0:0:180 8:0:0:1000'); presented('heading', 'missing')
        frame('7:0:0:! 11:0:0:180'); presented('heading', 'failed')
        frame('7:0:100:359')
        aged = presented('heading', value=359)
        require(int(aged['source_age_at_present_ms'][5:-1]) >= 100, 'Heading lost source age')
        presented('heading', 'stale')

        ok('instrument vsi')
        # Constant direct VSI despite altitude/baro changes: no numerical
        # derivative, indicated-altitude correction or stale heading fallback.
        for altitude, baro in [(1000, 29.92), (8500, 30.12)]:
            frame(f'1:0:0:{altitude} 3:0:0:{baro} 8:0:0:1000')
            presented('vsi', value=1000)
        frame('1:0:0:8500 3:0:0:30.12'); presented('vsi', 'missing')
        frame('8:0:0:!'); presented('vsi', 'failed')
        frame('8:0:100:-1000'); presented('vsi', value=-1000)
        presented('vsi', 'stale')
        result['status'] = ok('status')
        require('timing_fault=false' in result['status'], 'Owner timing fault')
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        try:
            for c in ['source sim', 'touch cancel', 'sim preset level', 'instrument altimeter', 'display recover', 'display on']:
                ok(c)
            result['final_memory'] = ok('memory status')
        finally:
            result['transcript'] = link.responses
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
            link.close()
    print('PASS Heading/VSI: local values, magnetic/direct-VSI isolation, invalidity, real ages and expiry.')


if __name__ == '__main__':
    main()
