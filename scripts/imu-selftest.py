"""Run QMI8658 electrostatic check-alive and retain the raw result/transcript."""
import argparse
import json
from pathlib import Path
import time
from cli import Link, require

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--port',default='COM5')
parser.add_argument('--report',type=Path,required=True)
args=parser.parse_args()
link=Link(args.port)
result={'passed':False}
try:
    ack=link.command('imu selftest','OK imu requested=selftest')
    request=dict(word.split('=',1) for word in ack.split() if '=' in word)['request']
    deadline=time.monotonic()+12
    while time.monotonic()<deadline:
        line=link.command('imu status')
        fields=dict(word.split('=',1) for word in line.split() if '=' in word)
        if fields.get('completed') != request:
            time.sleep(0.1)
            continue
        if 'state=failed' in line:
            raise RuntimeError(line)
        if 'selftest=passed' in line or 'selftest=failed' in line:
            result['status']=line
            result['passed']='selftest=passed' in line
            print(line)
            require(result['passed'], 'IMU check-alive did not meet QST thresholds')
            break
        time.sleep(0.1)
    else:
        raise TimeoutError('IMU self-test did not complete')
finally:
    result['transcript']=link.responses
    args.report.parent.mkdir(parents=True,exist_ok=True)
    args.report.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    link.close()
