"""Bench-IMU source isolation, display hold and completed-frame performance.

Hold the display upright; after the motion message, gently tilt/bank it.
This is receipt-age/transport verification, not calibration or flight validation.
"""
import argparse
import json
import time
from datetime import datetime, timezone
from pathlib import Path
from cli import Link, require

def fields(line):
    return dict(w.split('=', 1) for w in line.split() if '=' in w)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--port',default='COM5')
    p.add_argument('--seconds',type=int,default=60,choices=range(5,601))
    p.add_argument('--report',type=Path,required=True)
    args=p.parse_args()
    r={'passed':False,'timestamp_utc':datetime.now(timezone.utc).isoformat(),'seconds':args.seconds}
    l=Link(args.port)
    def ok(c): return l.command(c,'OK ')
    def wait(**expected):
        end=time.monotonic()+5
        while time.monotonic()<end:
            state=fields(ok('display status'))
            require(state['failed']=='false',str(state))
            if all(state[k]==v for k,v in expected.items()):return state
            time.sleep(.05)
        raise AssertionError(f'Timeout waiting for {expected}: {state}')
    try:
        ok('display off');ok('source imu')
        wait(renderer='attitude',shown_quality='valid',source='imu_bench')
        end=time.monotonic()+5
        while True:
            r['estimator']=ok('imu estimator')
            if 'quality=bench_only' in r['estimator']:break
            require(time.monotonic()<end,'Estimator did not complete a fresh averaging window')
            time.sleep(.05)
        require('quality=bench_only' in r['estimator'] and 'frame=BodyForwardRightDown' in r['estimator'],r['estimator'])
        for c in ['sim attitude 60','sim preset level','sim set pitch_deg 0']:
            l.command(c,'ERR unsupported')
        ok('display on');wait(backlight='true')
        r['hold_checks']=[]
        for stop,resume,quality in [('imu off','imu on','missing'),('fault roll_deg on','fault roll_deg off','failed')]:
            ok(stop);wait(shown_quality=quality,backlight='true')
            first=fields(ok('display sample'))
            first_status=fields(ok('display status'))
            require(first.get('holding')=='true' and first['shown_pitch_deg']!='None',str(first))
            time.sleep(1.2)
            second=fields(ok('display sample'))
            state=wait(shown_quality=quality,backlight='true')
            require(second['holding']=='true' and first['shown_pitch_deg']==second['shown_pitch_deg'] and first['shown_roll_deg']==second['shown_roll_deg'],'Held pose changed')
            age=lambda v:int(v['source_age_now_ms'].removeprefix('Some(').removesuffix(')'))
            require(age(second)>age(first)+1000,'Held source age did not advance')
            require(state['blanked_at_ms']==first_status['blanked_at_ms'],'Held display blanked')
            require(state['expired_ticks']==first_status['expired_ticks'],'Held display expired')
            r['hold_checks'].append({'stop':stop,'first':first,'second':second,'state':state})
            ok(resume);wait(shown_quality='valid',backlight='true')
            require(fields(ok('display sample'))['holding']=='false','Did not recover from hold')
        ok('instrument altimeter');wait(renderer='altimeter',shown_quality='missing',shown_alt_ft='None')
        ok('instrument attitude');wait(renderer='attitude',shown_quality='valid')
        time.sleep(1)
        ok('display on');wait(backlight='true')
        r['before']=wait(source='imu_bench',shown_quality='valid')
        r['imu_before']=ok('imu status')
        ok('display stats reset')
        print(f'IMU motion check running for {args.seconds}s. Gently tilt and bank the upright display.',flush=True)
        end=time.monotonic()+args.seconds
        while time.monotonic()<end:time.sleep(min(1,max(0,end-time.monotonic())))
        r['stats']=fields(ok('display stats stop'))
        r['after']=wait(source='imu_bench',backlight='true')
        for c in ['source status','imu status','display sample','status']:r[c]=ok(c)
        imu_before=fields(r['imu_before'])
        imu_after=fields(r['imu status'])
        r['unavailable_estimates']=int(imu_after['estimate_rejections'])-int(imu_before['estimate_rejections'])
        r['invalid_raw_tilts']=int(imu_after['raw_tilt_rejections'])-int(imu_before['raw_tilt_rejections'])
        require(float(r['stats']['rendered_fps'])>=30,'Below 30 completed FPS')
        require(r['before']['expired_ticks']==r['after']['expired_ticks'],'Numeric deadline missed')
        require(r['before']['blanked_at_ms']==r['after']['blanked_at_ms'],'Unexpected blanking')
        require('timing_fault=false' in r['status'],'Owner timing fault')
        require('errors=0' in r['imu status'],'Sensor bus errors')
        # Rejections are permitted: they retain the last usable pose without a visual annotation.
        # Raw source quality/age remain visible rather than being marked fresh.
        r['passed']=True
    except Exception as error:
        r['error']=repr(error)
        try:ok('display off')
        except Exception:pass
        raise
    finally:
        r['transcript']=l.responses
        args.report.parent.mkdir(parents=True,exist_ok=True)
        args.report.write_text(json.dumps(r,indent=2)+'\n',encoding='utf-8')
        l.close()
    print(json.dumps({k:v for k,v in r.items() if k!='transcript'},indent=2))

if __name__=='__main__':main()
