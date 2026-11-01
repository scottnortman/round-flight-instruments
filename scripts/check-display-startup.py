"""Minimal hardware regression for display-induced owner starvation."""
from pathlib import Path
import json
import time
from cli import Link, require

link=Link('COM5')
result={'passed':False}
try:
    deadline=time.monotonic()+4
    first=None
    while time.monotonic()<deadline:
        state=link.command('status','OK status')
        require('display=managed' in state,state)
        require('timing_fault=false' in state,state)
        display=link.command('display status','OK display')
        require('failed=false' in display,display)
        values=dict(x.split('=',1) for x in display.split() if '=' in x)
        frames=int(values['frames'])
        if first is None and frames>0:first=frames
        if first is not None and frames>=first+3:
            result['passed']=True
            print('PASS: display advances while owner remains healthy')
            break
        time.sleep(0.05)
    require(result['passed'],'No sustained display progress within four seconds')
except Exception as error:
    result['error']=repr(error)
    raise
finally:
    result['transcript']=link.responses
    Path('verification/display-startup-latest.json').write_text(json.dumps(result,indent=2)+'\n')
    link.close()
