"""Real LCD deadline-hold test: retain data on misses, blank only past the limit.

Deliberately stalls drawing/transfer, changes the owner data during the stall,
then verifies the old presented reading ages without renewal and recovery needs
an actually confirmed fresh LCD frame. No MSFS/aircraft writes. Restores default
three misses and level Altimeter, releases COM. Brief intentional blanking occurs.
"""
import argparse
import json
from pathlib import Path
import time
from cli import Link, require


def fields(line):
    return dict(w.split('=', 1) for w in line.split() if '=' in w)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--port', default='COM5')
    p.add_argument('--report', type=Path, required=True)
    args = p.parse_args()
    link = Link(args.port)
    ok = lambda c: link.command(c, 'OK ')
    result = {'passed': False, 'cases': []}

    def wait_ready():
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            d = fields(ok('display status'))
            if d['backlight'] == 'true' and d['shown_quality'] == 'valid' and d['consecutive_misses'] == '0':
                return d
        raise AssertionError('No fresh, physically presented frame: ' + str(d))

    try:
        for c in ['source sim', 'imu off', 'touch cancel', 'sim preset level',
                  'instrument attitude', 'display recover', 'display gauge', 'display on']:
            ok(c)
        require(fields(ok('display hold status'))['allowed_misses'] == '3', 'Default is not three')
        for limit, stall in [(0, 'stall'), (1, 'stall'), (3, 'stall'), (3, 'transfer-stall')]:
            for c in [f'display hold {limit}', 'sim set pitch_deg 10', 'sim set roll_deg 20']:
                ok(c)
            time.sleep(.3)
            before = wait_ready()
            before_hold = fields(ok('display hold status'))
            ok('display ' + stall)
            # Fresh owner values must not magically change the retained LCD data.
            ok('sim set pitch_deg -10'); ok('sim set roll_deg -20')
            observations = []
            old_sample = None
            deadline = time.monotonic() + 1
            while time.monotonic() < deadline:
                d = fields(ok('display status')); observations.append(d)
                require(d['failed'] == 'false', 'Display worker failed')
                if d['deadline_holding'] == 'true' and old_sample is None:
                    line = link.command('display sample')
                    if line.startswith('OK '):
                        old_sample = fields(line)
                if d['backlight'] == 'false':
                    break
            else:
                raise AssertionError('Sustained miss did not blank')
            hold = fields(ok('display hold status'))
            item = {'limit': limit, 'stall': stall, 'before': before, 'before_hold': before_hold,
                    'observations': observations, 'dark': d, 'hold': hold, 'held_sample': old_sample}
            result['cases'].append(item)
            require(int(d['last_blank_misses']) == limit + 1, 'Did not blank on X+1: ' + str(d))
            require(int(d['deadline_blanks']) == int(before['deadline_blanks']) + 1, 'Wrong blank cause')
            require(int(hold['fresh_presentations']) >= int(before_hold['fresh_presentations']), 'Counter moved backward')
            active = [o for o in observations if o['deadline_holding'] == 'true']
            if limit:
                require(active and all(o['backlight'] == 'true' for o in active), 'Permitted miss blanked')
                require(old_sample is not None and old_sample['shown_pitch_deg'] == 'Some(10.0)'
                        and old_sample['shown_roll_deg'] == 'Some(20.0)', 'Last reading was not retained')
                require(old_sample['holding'] == 'true' and old_sample['quality'] == 'held', 'Hold hidden in diagnostics')
                require(int(old_sample['source_age_now_ms'][5:-1]) > 100, 'Old sample age was renewed')
            else:
                require(not active, 'Zero-limit mode still held')
            ok('display recover')
            after = wait_ready()
            after_hold = fields(ok('display hold status'))
            require(int(after_hold['fresh_presentations']) > int(hold['fresh_presentations']),
                    'Recovered without a confirmed fresh LCD frame')
            require(after_hold['consecutive_misses'] == '0' and after_hold['active'] == 'false', 'Run did not reset')
            item.update(after=after, after_hold=after_hold)
            print(f'PASS {stall}, X={limit}: blank at miss {limit + 1}, fresh-LCD recovery', flush=True)
        # A generous numeric hold cannot disable the independent render guard.
        for c in ['display hold 10', 'sim preset level', 'instrument altimeter']:
            ok(c)
        time.sleep(.3)
        before = wait_ready()
        ok('display stall')
        until = time.monotonic() + 1
        while time.monotonic() < until:
            dark = fields(ok('display status'))
            if dark['backlight'] == 'false':
                break
        else:
            raise AssertionError('Independent render guard was masked')
        require(dark['last_blank_misses'] == '0' and dark['deadline_blanks'] == before['deadline_blanks'],
                'Render failure was misclassified as numeric hold exhaustion')
        require(int(dark['frame_age_ms']) >= 250, 'Render blanking timing is inconsistent')
        result['independent_render_guard'] = {'before': before, 'dark': dark}
        ok('display recover'); wait_ready()
        print('PASS independent render guard with X=10', flush=True)
        result['owner'] = ok('status')
        require('timing_fault=false' in result['owner'], 'Owner timing fault')
        result['passed'] = True
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        try:
            for c in ['display recover', 'display hold 3', 'source sim', 'touch cancel',
                      'sim preset level', 'instrument altimeter', 'display on']:
                ok(c)
        finally:
            result['transcript'] = link.responses
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(result, indent=2) + '\n')
            link.close()


if __name__ == '__main__':
    main()
