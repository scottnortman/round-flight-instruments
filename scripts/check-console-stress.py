"""Exercise the real owner command queues; fail on reset, timing fault or timeout."""
import argparse
import json
from pathlib import Path
from cli import Link, require


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--port', default='COM5')
    p.add_argument('--requests', type=int, default=1200)
    p.add_argument('--report', type=Path, required=True)
    args = p.parse_args()
    if not 1 <= args.requests <= 10000:
        p.error('requests must be 1..10000')
    link = Link(args.port)
    result = {'passed': False, 'target': args.port, 'requests': args.requests}
    last_tick = None
    try:
        for n in range(args.requests):
            command = ('status', 'touch status', 'display status')[n % 3]
            response = link.command(command, 'OK ')
            if command == 'status':
                tick = int(response.split('ticks=')[1].split()[0])
                require(last_tick is None or tick >= last_tick, 'board reset: ticks regressed')
                require('timing_fault=false' in response, response)
                last_tick = tick
            elif command == 'display status':
                require('failed=false' in response, response)
            if (n + 1) % 300 == 0:
                print(f'{n + 1} requests completed', flush=True)
        result.update(passed=True, max_response_ms=max(r['elapsed_ms'] for r in link.responses))
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        result['transcript'] = link.responses
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        link.close()
    print(json.dumps({k: v for k, v in result.items() if k != 'transcript'}, indent=2))


if __name__ == '__main__':
    main()
