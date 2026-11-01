"""Observe a running simulated altimeter without changing board state."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import time
from cli import Link, require


def fields(line):
    return dict(word.split('=', 1) for word in line.split() if '=' in word)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='COM5')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    link = Link(args.port)
    result = {'passed': False, 'timestamp_utc': datetime.now(timezone.utc).isoformat()}
    start = time.monotonic()
    first = None
    try:
        while True:
            state = fields(link.command('status', 'OK status'))
            display = fields(link.command('display status', 'OK display'))
            require(state['timing_fault'] == 'false', str(state))
            for key, value in dict(ready='true', failed='false', backlight='true',
                                   mode='gauge', stall='0', shown_quality='valid',
                                   renderer='altimeter').items():
                require(display[key] == value, str(display))
            if first is None:
                first = display
                first_at = time.monotonic()
            else:
                require(display['blanked_at_ms'] == first['blanked_at_ms'],
                        'Unexpected guard blanking')
            if time.monotonic() - start >= 20:
                elapsed = time.monotonic() - first_at
                frames = int(display['frames']) - int(first['frames'])
                require(frames > 0, 'No presentation progress')
                result.update(passed=True, observed_seconds=round(elapsed, 2),
                              presented_frames=frames, rendered_fps=round(frames / elapsed, 2),
                              final_display=display, final_status=state,
                              imu_status=link.command('imu status', 'OK imu'))
                break
            time.sleep(0.5)
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        result['transcript'] = link.responses
        args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        link.close()
    print(json.dumps({key: value for key, value in result.items() if key != 'transcript'}, indent=2))


if __name__ == '__main__':
    main()
