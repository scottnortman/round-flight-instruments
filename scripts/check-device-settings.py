"""Bench persistence/identity regression; changes saved gauge and restarts the board.

Requires pyserial and esptool. Backs up the original UUID in the report, restores
it after the provisioning test, and finishes on saved Altimeter at 4520/29.92.
Optional --stress-writes exercises NVS page turnover while switching all six
gauges; it consumes a small number of flash erase cycles. No simulator writes.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time
import uuid
from cli import Link, require


def fields(line):
    return dict(w.split('=', 1) for w in line.split() if '=' in w)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--port', default='COM5')
    p.add_argument('--report', type=Path, required=True)
    p.add_argument('--stress-writes', type=int, default=0)
    args = p.parse_args()
    require(0 <= args.stress_writes <= 500, 'Stress bound is 0..500')
    result = {'passed': False, 'reboots': [], 'checks': []}
    link = None
    original = None

    def ok(command):
        deadline = time.monotonic() + 2
        while True:
            response = link.command(command)
            if response == 'ERR settings_busy' and time.monotonic() < deadline:
                time.sleep(.03)
                continue
            require(response.startswith('OK '), command + ': ' + response)
            return response

    def wait_saved(gauge):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            status = fields(ok('settings status'))
            require(status['failures'] == '0', 'NVS failure: ' + str(status))
            if status['dirty'] == 'false' and status['saved'] == gauge:
                return status
            time.sleep(.03)
        raise AssertionError('Save did not complete: ' + str(status))

    def wait_identity(value):
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            status = fields(ok('settings status'))
            require(status['error'] == '0', 'Identity save failed: ' + str(status))
            if status['uuid_pending'] == 'false' and fields(ok('device info'))['uuid'] == value:
                return
            time.sleep(.03)
        raise AssertionError('Identity did not commit')

    def reconnect_after_restart(gauge, expected_id):
        nonlocal link
        link.close(); link = None
        reset = subprocess.run([sys.executable, '-m', 'esptool', '--chip', 'esp32s3',
                                '--port', args.port, '--baud', '460800', 'flash_id'],
                               capture_output=True, text=True, timeout=20)
        require(reset.returncode == 0, reset.stdout + reset.stderr)
        time.sleep(2)
        link = Link(args.port)
        identity = fields(ok('device info'))
        status = fields(ok('status'))
        saved = fields(ok('settings status'))
        result['reboots'].append({'expected': gauge, 'identity': identity, 'status': status, 'settings': saved})
        require(identity['uuid'] == expected_id and identity['identity_persisted'] == 'true', 'UUID changed across reboot')
        require(status['instrument'] == gauge and saved['saved'] == gauge, 'Selection not restored')
        require(status['source'] == 'sim' and status['timing_fault'] == 'false', 'Runtime state restored or faulted')
        require(':valid:' not in ok('status'), 'Boot restored old numeric readings')
        require(saved['writes'] == '0' and saved['failures'] == '0', 'Boot rewrote unchanged records')

    try:
        link = Link(args.port)
        original = fields(ok('device info'))['uuid']
        require(original != 'unavailable', 'Missing provisioned UUID')
        uuid.UUID(original)
        result['original_uuid'] = original
        for c in ['source sim', 'imu off', 'touch cancel']:
            ok(c)
        for gauge in ['attitude', 'turn', 'airspeed', 'heading', 'vsi', 'altimeter']:
            ok('instrument ' + gauge)
            wait_saved(gauge)  # automatic save, no explicit flush
            reconnect_after_restart(gauge, original)

        # Reissuing the current choice must not consume flash writes.
        before = fields(ok('settings status'))
        for _ in range(8):
            ok('instrument altimeter'); ok('settings save')
        time.sleep(1.2)
        after = fields(ok('settings status'))
        require(before['writes'] == after['writes'], 'Unchanged choice caused flash writes')
        result['checks'].append({'unchanged_before': before, 'unchanged_after': after})

        # An unchanged manual flush must not force the NEXT choice into flash.
        ok('instrument attitude'); time.sleep(.15)
        early = fields(ok('settings status'))
        require(early['dirty'] == 'true' and early['writes'] == after['writes'], 'Unchanged flush leaked into a later choice')
        wait_saved('attitude')
        before = fields(ok('settings status'))
        for gauge in ['turn', 'heading', 'vsi', 'airspeed', 'altimeter']:
            ok('instrument ' + gauge)
        after = wait_saved('altimeter')
        require(int(after['writes']) == int(before['writes']) + 1, 'Rapid choices did not coalesce')
        result['checks'].append({'debounced': early, 'coalesced_before': before, 'coalesced_after': after})

        replacement = str(uuid.uuid4())
        ok('device uuid set ' + replacement); wait_identity(replacement)
        reconnect_after_restart('altimeter', replacement)
        ok('device uuid set ' + original); wait_identity(original)
        reconnect_after_restart('altimeter', original)
        require(link.command('device uuid set 00000000-0000-0000-0000-000000000000').startswith('ERR '), 'Nil accepted')

        # A connected source must reject re-identification without touching flash.
        ok('source external2 synthetic 987654321')
        require(link.command('device uuid set ' + replacement).startswith('ERR device_busy'), 'Active-source identity changed')
        ok('source sim')
        require(fields(ok('device info'))['uuid'] == original, 'Rejected identity changed')

        for c in ['sim preset level', 'display gauge', 'display on']:
            ok(c)
        time.sleep(1)
        ok('display stats reset')
        before = fields(ok('display status'))
        last_tick = int(fields(ok('status'))['ticks'])
        writes_before = int(fields(ok('settings status'))['writes'])
        gauges = ['attitude', 'turn', 'airspeed', 'heading', 'vsi', 'altimeter']
        stress = {'before': before, 'observations': []}
        result['stress'] = stress
        for index in range(args.stress_writes):
            gauge = gauges[index % len(gauges)]
            ok('instrument ' + gauge); ok('settings save')
            settings = wait_saved(gauge)
            d = fields(ok('display status'))
            require(d['blanked_at_ms'] == before['blanked_at_ms'], 'Settings write blanked the screen')
            require(d['failed'] == 'false', 'Display worker failed')
            owner = fields(ok('status'))
            require(owner['timing_fault'] == 'false', 'State owner deadline missed')
            require(int(owner['ticks']) >= last_tick, 'Unexpected reset during save stress')
            last_tick = int(owner['ticks'])
            if index % 25 == 0:
                stress['observations'].append({'index': index, 'display': d, 'settings': settings})
        ok('display stats stop')
        stress['after'] = fields(ok('display status'))
        stress['stats'] = ok('display stats')
        stress['settings'] = fields(ok('settings status'))
        require(int(stress['settings']['writes']) == writes_before + args.stress_writes, 'Unexpected number of committed saves')
        result['memory'] = ok('memory status')
        result['passed'] = True
    finally:
        if link:
            try:
                if original and fields(ok('device info'))['uuid'] != original:
                    ok('source sim'); ok('device uuid set ' + original); wait_identity(original)
                for c in ['source sim', 'sim preset level', 'instrument altimeter', 'settings save']:
                    ok(c)
                wait_saved('altimeter')
            finally:
                link.close()
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'passed': result['passed'], 'reboots': len(result['reboots']),
                      'stress_writes': args.stress_writes, 'stats': result['stress']['stats']}, indent=2))


if __name__ == '__main__':
    main()
