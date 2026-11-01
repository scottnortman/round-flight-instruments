"""Capture the opt-in watchdog test build; requires subsequent normal reflash."""
import argparse
from pathlib import Path
import time
import serial
from cli import UART_BAUD

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--port', default='COM5')
parser.add_argument('--baud', type=int, default=UART_BAUD)
parser.add_argument('--log', type=Path, required=True)
args = parser.parse_args()
port = serial.Serial(port=None, baudrate=args.baud, timeout=0.2)
port.dtr = False
port.rts = False
port.port = args.port
port.open()
data = bytearray()
try:
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline and len(data) < 100_000:
        data.extend(port.read(2048))
finally:
    port.close()
log = data.decode('utf-8', errors='replace')
args.log.parent.mkdir(parents=True, exist_ok=True)
args.log.write_text(log, encoding='utf-8')
if not ('Task watchdog got triggered' in log and 'main' in log
        and 'Rebooting...' in log and 'READY protocol=1' in log):
    raise RuntimeError(f'Watchdog panic + reboot + ready sequence not captured: {args.log}')
print('PASS: owner stall produced a task-watchdog panic and reboot to READY.')
