"""Board T2 transport/render regression; keeps every measurement's real age.

Synthetic bench input only. Run with the IDF Python (pyserial required).
"""
import argparse,json,math,time,zlib
from pathlib import Path
from cli import Link

def fields(s): return dict(w.split('=',1) for w in s.split() if '=' in w)
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--port',default='COM5');p.add_argument('--seconds',type=int,default=30)
    p.add_argument('--report',type=Path,required=True)
    p.add_argument('--gauges',nargs='+',choices=['altimeter','attitude','turn','airspeed','heading','vsi'],default=['altimeter','attitude','altimeter'])
    a=p.parse_args()
    if not 1 <= a.seconds <= 300: p.error('--seconds must be 1..300')
    import ctypes
    assert ctypes.windll.winmm.timeBeginPeriod(1)==0
    link=Link(a.port);report={'passed':False,'gauges':[]}
    max_ack_ms=0.0
    sid=time.time_ns()//1000;seq=0
    try:
        caps=link.command('protocol capabilities','OK capabilities');assert 'T2' in caps
        at=int(fields(link.command(f'source external2 synthetic {sid}','OK source'))['now_ms'])
        def feed():
            nonlocal seq,at,max_ack_ms
            seq+=1;t=time.perf_counter()
            values=[4520+1000*math.sin(t/10),4500,29.92,20*math.sin(t/2),40*math.sin(t/4),120,270,500,3*math.sin(t/2),.8*math.sin(t/3)]
            body=f'T2 {sid} {seq} {at} running' + ''.join(f' {i}:0:1:{v:.5f}' for i,v in enumerate(values,1))
            began=time.perf_counter()
            ack=link.command(f'{body} *{zlib.crc32(body.encode()):08X}','OK telemetry')
            max_ack_ms=max(max_ack_ms,(time.perf_counter()-began)*1000)
            at=int(fields(ack)['now_ms'])
        for gauge in a.gauges:
            link.command(f'instrument {gauge}','OK ')
            warm=time.perf_counter()+1
            while time.perf_counter()<warm: feed()
            before=fields(link.command('display status','OK '));feed()
            link.command('display stats reset','OK ');feed()
            deadline=time.perf_counter()+a.seconds
            while time.perf_counter()<deadline: feed()
            after=fields(link.command('display status','OK '));feed()
            stats=fields(link.command('display stats stop','OK '));feed()
            item={'gauge':gauge,'before':before,'after':after,'stats':stats}
            report['gauges'].append(item);print(json.dumps(item),flush=True)
            assert after['failed']=='false' and after['shown_quality']=='valid',item
            assert float(stats['rendered_fps'])>=30,item
            assert before['blanked_at_ms']==after['blanked_at_ms'],item
            assert before['expired_ticks']==after['expired_ticks'],item
        report['passed']=True
    finally:
        report['frames']=seq
        report['max_ack_ms']=max_ack_ms
        a.report.parent.mkdir(parents=True,exist_ok=True)
        a.report.write_text(json.dumps(report,indent=2)+'\n')
        # Return to an explicit local bench preset, not an automatic fallback.
        link.command('source sim');link.command('sim preset level');link.command('instrument altimeter')
        link.close()
        ctypes.windll.winmm.timeEndPeriod(1)
if __name__=='__main__':main()
