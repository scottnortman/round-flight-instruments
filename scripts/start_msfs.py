"""One-command live test with exclusive, unique evidence files. No shell redirection."""
import argparse
from datetime import datetime
from pathlib import Path
import subprocess
import sys
import uuid


def reserve_run(root):
    root.mkdir(parents=True, exist_ok=True)
    folder = root / (datetime.now().strftime('%Y%m%d-%H%M%S-') + uuid.uuid4().hex[:8])
    folder.mkdir()  # Never reuse a capture directory or overwrite existing evidence.
    return folder


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', required=True)
    parser.add_argument('--writeback', action='store_true')
    parser.add_argument('--frame-timing', action='store_true')
    parser.add_argument('--logs', type=Path, default=Path('logs'))
    args = parser.parse_args()
    folder = reserve_run(args.logs).resolve()
    print(f'Logs: {folder}', flush=True)
    command = [sys.executable, '-u', str(Path(__file__).with_name('sim_bridge.py')),
               '--msfs', '--port', args.port, '--diagnostics',
               '--log', str(folder / 'frames.jsonl'), '--report', str(folder / 'summary.json')]
    if args.writeback:
        command.append('--writeback')
    if args.frame_timing:
        command.append('--frame-timing')
    with (folder / 'console.txt').open('x', encoding='utf-8') as log:
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   text=True, encoding='utf-8', errors='replace')
        try:
            for line in process.stdout:
                print(line, end='', flush=True)
                log.write(line)
                log.flush()
        except KeyboardInterrupt:
            # Ctrl+C reaches the child in the same console group. Allow it to
            # flush its summary and close the device; never restart/replay writes.
            try:
                tail, _ = process.communicate(timeout=5)
                if tail:
                    log.write(tail)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=3)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=3)
            process.stdout.close()
    return process.returncode


if __name__ == '__main__':
    raise SystemExit(main())
