"""Test-only SimConnect DLL -> packaged worker/connector -> actual UART board.

No simulator needed. Fixture data is not live aircraft data. Production DLL and
executables are never replaced. Close the tray/terminal first. Includes deliberate
source-loss tests that SHOULD briefly invalidate the gauge. Requires Windows,
MSVC, MSFS SDK headers, pyserial, and current firmware/connector.
"""
import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from cli import Link, require

ROOT = Path(__file__).resolve().parents[1]
HIDDEN = getattr(subprocess, 'CREATE_NO_WINDOW', 0)


def fields(line):
    return dict(w.split('=', 1) for w in line.split() if '=' in w)


def build_fixture(out, sdk):
    vswhere = Path(os.environ['ProgramFiles(x86)']) / 'Microsoft Visual Studio/Installer/vswhere.exe'
    vs = subprocess.check_output([str(vswhere), '-latest', '-products', '*', '-property', 'installationPath'], text=True).strip()
    vcvars = Path(vs) / 'VC/Auxiliary/Build/vcvars64.bat'
    dll = out / 'TEST-ONLY-SimConnect-Timing.dll'
    batch = out / 'build.cmd'
    batch.write_text(f'@echo off\ncall "{vcvars}" >nul\nif errorlevel 1 exit /b 1\n'
                     f'cl /nologo /LD /MT /EHsc /std:c++17 /O2 /I"{sdk / "SimConnect SDK/include"}" '
                     f'"{ROOT / "tests/fixtures/simconnect_timing.cpp"}" /link /OUT:"{dll}"\n', encoding='utf-8')
    result = subprocess.run(['cmd.exe', '/d', '/c', str(batch)], cwd=out, capture_output=True, text=True, creationflags=HIDDEN)
    (out / 'build.log').write_text(result.stdout + result.stderr)
    require(result.returncode == 0, 'Fixture build failed; see build.log')
    return dll


def constrained_children(command, env, output, loaded, above_normal):
    """Launch connector/worker and two competitors on two logical CPUs only."""
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.GetProcessAffinityMask.argtypes = [wintypes.HANDLE, ctypes.POINTER(ctypes.c_size_t), ctypes.POINTER(ctypes.c_size_t)]
    kernel.SetProcessAffinityMask.argtypes = [wintypes.HANDLE, ctypes.c_size_t]
    handle = kernel.GetCurrentProcess()
    original, system = ctypes.c_size_t(), ctypes.c_size_t()
    require(kernel.GetProcessAffinityMask(handle, ctypes.byref(original), ctypes.byref(system)), 'Cannot read CPU affinity')
    cpus = [1 << i for i in range(64) if original.value & (1 << i)][:2]
    require(len(cpus) == 2, 'Host stress requires two available logical CPUs')
    mask = cpus[0] | cpus[1]
    competitors = []
    process = None
    try:
        if loaded:
            require(kernel.SetProcessAffinityMask(handle, mask), 'Cannot constrain test CPU affinity')
            # Two normal-priority CPU-bound processes compete with the connector
            # and worker on these two CPUs. Other host CPUs remain unrestricted.
            for _ in range(2):
                competitors.append(subprocess.Popen([sys.executable, '-c',
                    'import time; end=time.monotonic()+180; x=1\nwhile time.monotonic()<end: x=(x*1664525+1013904223)&0xffffffff'],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=HIDDEN))
        process = subprocess.Popen(command, env=env, stdout=output, stderr=subprocess.STDOUT,
                                   creationflags=HIDDEN | (0x8000 if above_normal else 0))
        if loaded:
            for child in [process] + competitors:
                actual, available = ctypes.c_size_t(), ctypes.c_size_t()
                require(kernel.GetProcessAffinityMask(int(child._handle), ctypes.byref(actual), ctypes.byref(available)), 'Cannot verify child CPU affinity')
                require(actual.value == mask, 'CPU competitor/connector did not inherit constrained affinity')
    except BaseException:
        for child in ([process] if process is not None else []) + competitors:
            child.terminate(); child.wait()
        raise
    finally:
        kernel.SetProcessAffinityMask(handle, original.value)
    return process, competitors, mask if loaded else original.value


def run_case(args, dll, name, gauge, mode, loaded, duration):
    folder = args.report.parent / name
    folder.mkdir()
    link = Link(args.port)
    try:
        # Off is idempotent; a busy nonblocking IMU mailbox is retried only in
        # setup, before timing begins. Never replay controls/telemetry this way.
        for attempt in range(10):
            response=link.command('imu off')
            if response.startswith('OK '): break
            require(response=='ERR imu_busy','Cannot disable IMU diagnostics: '+response)
            time.sleep(.05)
        else:
            raise AssertionError('IMU setup remained busy')
        for c in ['source sim', 'sim preset level', 'touch cancel', 'instrument ' + gauge, 'display on']:
            link.command(c, 'OK ')
        time.sleep(.3)
        before = fields(link.command('display status', 'OK '))
        memory_before = fields(link.command('memory status', 'OK '))
        link.command('display stats reset', 'OK ')
    finally:
        link.close()
    env = os.environ.copy()
    state = folder / 'opens.txt'
    env['RFI_TIMING_FIXTURE'] = str(state.resolve())
    env['RFI_TIMING_MODE'] = mode
    if args.above_normal:
        env['RFI_TIMING_PRIORITY'] = '1'
    else:
        env.pop('RFI_TIMING_PRIORITY', None)
    command = [str(args.connector.resolve()), 'run', '--config', str(args.config.resolve()),
               '--msfs', '--dll', str(dll), '--sixpack', '--app-status', '--writeback',
               '--duration', str(duration), '--log-dir', str(folder / 'logs')]
    # Only fault cases use extra probes: steady performance matches tray options.
    if mode not in ['jitter', 'short_gap']:
        command += ['--diagnostics']
    with (folder / 'console.log').open('w', encoding='utf-8') as output:
        process, competitors, affinity = constrained_children(command, env, output, loaded, args.above_normal)
        try:
            code = process.wait(timeout=duration + 15)
        finally:
            if process.poll() is None:
                process.terminate(); process.wait()
            for child in competitors:
                child.terminate(); child.wait()
    link = Link(args.port)
    try:
        stats = fields(link.command('display stats stop', 'OK '))
        after = fields(link.command('display status', 'OK '))
        memory_after = fields(link.command('memory status', 'OK '))
        owner = link.command('status', 'OK ')
    finally:
        link.close()
    session_folder = next((folder / 'logs').iterdir())
    summary = json.loads((session_folder / 'summary.json').read_text())
    records = [json.loads(line) for p in sorted(session_folder.glob('events-*.jsonl')) for line in p.read_text().splitlines()]
    frames = [r for r in records if 'frame' in r]
    numeric = [r for r in frames if r['frame'].split()[4] == 'running'
               and any(w.startswith('4:0:') and len(w.split(':')) == 4
                       and w.split(':')[3] not in ['-', '!', '?'] for w in r['frame'].split()[5:-1])]
    expiry = int(after['expired_source_valid']) - int(before['expired_source_valid'])
    result = {'name': name, 'gauge': gauge, 'mode': mode, 'loaded': loaded,
              'test_forced_above_normal': args.above_normal,
              'affinity_mask': affinity, 'competitors': 2 if loaded else 0,
              'returncode': code, 'summary': summary, 'before': before, 'after': after,
              'stats': stats, 'memory_before': memory_before, 'memory_after': memory_after,
              'valid_source_expiry_delta': expiry}
    # Save evidence before assertions, including all negative outcomes.
    (folder / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    require(code == 0 and summary['end'] == 'stopped', 'Connector failed: ' + str(folder))
    require(after['failed'] == 'false' and 'timing_fault=false' in owner, 'Board/owner fault')
    require(before['blanked_at_ms'] == after['blanked_at_ms'], 'Data transition blanked the screen')
    require(len(numeric) >= duration * 10, 'Too few actual MSFS-path numeric frames')
    require(not Path(str(state) + '.writes').exists(), 'Unexpected simulator write without user action')
    require(memory_before['psram_free_bytes'] == memory_after['psram_free_bytes'], 'PSRAM allocation changed')
    if mode in ['jitter', 'short_gap']:
        require(float(stats['rendered_fps']) >= 30, 'Below 30 completed FPS')
        require(expiry == 0, 'Unexpected valid-source expiry')
        require(int(stats.get('warning_frames', 0)) == 0, 'An invalid-data warning interrupted the running gauge')
        require(summary['sessions'] == 1, 'Unexpected reconnect during normal timing')
        if mode == 'short_gap':
            require(int(stats.get('repeated_frames', 0)) > 0, 'Short gap did not exercise completed-frame reuse')
    else:
        probes = [r[k] for r in records for k in ['diagnostic', 'session_probe'] if k in r]
        recovered = [p for p in probes if p['elapsed_ms'] > (11500 if mode == 'switch' else 6500)
                     and 'shown_quality=valid' in p.get('display', '')]
        require(recovered, 'No physically presented valid view confirmed after fault recovery')
        if mode == 'switch':
            require(summary['sessions'] == 3 and int(state.read_text()) == 3, 'Did not automatically recover both aircraft changes')
        elif mode == 'failed':
            require(any('shown_quality=failed' in p.get('display', '') for p in probes),
                    'Explicit instrument failure was hidden by continuity')
        else:
            # Diagnostics are deliberately sparse and may miss the short stale
            # view. The independent guard also records every numeric expiry,
            # including when the current source has already become invalid.
            require(int(stats.get('warning_frames', 0)) > 0
                    or int(after['expired_ticks']) > int(before['expired_ticks'])
                    or any('shown_quality=stale' in p.get('display', '') for p in probes),
                    'Deliberate 800 ms source gap did not exercise invalid-data handling')
            require(any(int(w.split(':')[2]) >= 800 for r in numeric for w in r['frame'].split()[5:-1] if w.startswith('4:0:')),
                    'Delayed readings lost their real age')
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--port', default='COM5')
    p.add_argument('--config', type=Path, required=True)
    p.add_argument('--connector', type=Path, required=True)
    p.add_argument('--sdk', type=Path, default=Path('C:/MSFS SDK'))
    p.add_argument('--seconds', type=int, choices=range(10, 61), default=20)
    p.add_argument('--cases', nargs='+', help='optional case names, for a targeted repeat')
    p.add_argument('--above-normal', action='store_true', help='test-only scheduling experiment; production binaries unchanged')
    p.add_argument('--report', type=Path, required=True, help='use a new output directory for every run')
    args = p.parse_args()
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = {'passed': False, 'test_only_simulator': True, 'reply_delay_ms': [10, 40], 'cases': []}
    print('TEST ONLY: the screen will show fixture data through the MSFS path, not a real aircraft.', flush=True)
    try:
        dll = build_fixture(args.report.parent.resolve(), args.sdk)
        for loaded in [False, True]:
            for gauge in ['altimeter', 'attitude', 'turn']:
                name = ('loaded-' if loaded else 'normal-') + gauge
                if args.cases and name not in args.cases:
                    continue
                print('Running ' + name, flush=True)
                try:
                    result = run_case(args, dll, name, gauge, 'jitter', loaded, args.seconds)
                    result['passed'] = True
                    print(f"PASS {name}: {result['stats']['rendered_fps']} completed FPS, zero valid-source expiry", flush=True)
                except AssertionError as error:
                    result_file=args.report.parent / name / 'result.json'
                    result = json.loads(result_file.read_text()) if result_file.exists() else {'name':name,'stage':'setup'}
                    result.update(passed=False, error=str(error))
                    print(f'FAIL {name}: {error}', flush=True)
                report['cases'].append(result)
        for gauge, mode in [('attitude', 'switch'), ('attitude', 'short_gap'), ('turn', 'short_gap'), ('attitude', 'gap'), ('turn', 'gap'), ('attitude', 'failed'), ('turn', 'failed')]:
            name = gauge + '-' + mode
            if args.cases and name not in args.cases:
                continue
            print(f'Running {gauge}-{mode}: ' + ('repeat without blanking expected' if mode == 'short_gap' else 'brief invalid indication is EXPECTED'), flush=True)
            try:
                result = run_case(args, dll, name, gauge, mode, False, 15)
                result['passed'] = True
                print(f'PASS {gauge}-{mode}: automatic recovery, no unintended aircraft writes', flush=True)
            except AssertionError as error:
                result_file=args.report.parent / name / 'result.json'
                result = json.loads(result_file.read_text()) if result_file.exists() else {'name':name,'stage':'setup'}
                result.update(passed=False, error=str(error))
                print(f'FAIL {name}: {error}', flush=True)
            report['cases'].append(result)
        report['passed'] = bool(report['cases']) and all(c['passed'] for c in report['cases'])
    except Exception as error:
        report['error'] = repr(error)
        raise
    finally:
        args.report.write_text(json.dumps(report, indent=2) + '\n')
        link = Link(args.port)
        try:
            for c in ['source sim', 'sim preset level', 'touch cancel', 'baro 29.92', 'instrument altimeter']:
                link.command(c, 'OK ')
        finally:
            link.close()
    if not report['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
