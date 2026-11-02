"""Exercise the real display path and independent backlight supervisor over UART.

Temporarily stalls rendering and pre-submission transfer work. Success leaves the
simulated altimeter visible at 4520 ft/29.92; failure requests display off.
Physical appearance and electrical blanking still require human observation.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import time
from cli import Link, require


def fields(line):
    return dict(word.split('=',1) for word in line.split() if '=' in word)


def wait(link, **expected):
    deadline=time.monotonic()+4
    while time.monotonic()<deadline:
        line=link.command('display status','OK display')
        data=fields(line)
        require(data['failed']=='false',line)
        if all(data[k]==v for k,v in expected.items()):
            return data
        time.sleep(0.05)
    raise TimeoutError(f'Display did not reach {expected}')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port',default='COM5')
    parser.add_argument('--report',type=Path,required=True)
    args=parser.parse_args()
    link=Link(args.port)
    result={'passed':False,'timestamp_utc':datetime.now(timezone.utc).isoformat()}
    try:
        require('display=managed' in link.command('status','OK status'),'Board must report managed display')
        for cmd in ('display recover','display on','display gauge','instrument altimeter','sim resume','baro 29.92','fault pressure_altitude_ft off','sim clear pressure_altitude_ft'):
            link.command(cmd,'OK ')
        wait(link,ready='true',backlight='true',shown_quality='missing')
        link.command('sim set pressure_altitude_ft 0','OK ')
        zero=wait(link,shown_quality='valid',shown_alt_ft='Some(0.0)')
        require(int(zero['psram_bytes'])>=2*480*480*2,'Insufficient PSRAM')
        for value in ('4520','-100','1000','60000'):
            link.command('sim set pressure_altitude_ft '+value,'OK ')
            wait(link,shown_quality='valid',shown_alt_ft=f'Some({float(value)})')
        link.command('sim set pressure_altitude_ft NaN','ERR number')
        wait(link,shown_alt_ft='Some(60000.0)')
        link.command('fault pressure_altitude_ft on','OK ')
        wait(link,shown_quality='failed',shown_alt_ft='None')
        link.command('fault pressure_altitude_ft off','OK ')
        link.command('sim preset level','OK ')
        wait(link,shown_quality='valid',shown_alt_ft='Some(4520.0)')
        link.command('sim pause','OK ')
        wait(link,shown_quality='stale',shown_alt_ft='None')
        link.command('sim resume','OK ')
        wait(link,shown_quality='valid')
        for stall in ('stall','transfer-stall'):
            before=wait(link,backlight='true')
            start=time.monotonic()
            link.command('display '+stall,'OK ')
            dark=wait(link,backlight='false')
            elapsed=time.monotonic()-start
            require(elapsed<0.6,'Blanking not observed within 600ms host polling budget')
            first=fields(link.command('status','OK status'))
            time.sleep(0.3)
            second=fields(link.command('status','OK status'))
            require(int(second['ticks'])>int(first['ticks']), 'Owner stopped with display')
            require(second['timing_fault']=='false','Owner timing fault')
            result[stall]={'blank_observed_ms':round(elapsed*1000,1),'before':before,'dark':dark}
            link.command('display recover','OK ')
            wait(link,backlight='true',shown_quality='valid')
        link.command('instrument airspeed','OK ')
        wait(link,shown_quality='placeholder',shown_alt_ft='None')
        link.command('instrument altimeter','OK ')
        wait(link,shown_quality='valid',shown_alt_ft='Some(4520.0)')
        link.command('baro 30.92','OK ')
        time.sleep(0.3)
        baro=fields(link.command('display status','OK '))
        corrected=float(baro['shown_alt_ft'][5:-1])
        require(5430<corrected<5434,'Incorrect simulated barometric adjustment')
        link.command('baro 29.92','OK ')
        wait(link,shown_alt_ft='Some(4520.0)')
        link.command('display off','OK ')
        wait(link,backlight='false')
        link.command('display on','OK ')
        result['final_display']=wait(link,backlight='true')
        result['final_status']=link.command('status','OK status')
        result['passed']=True
    except Exception as error:
        result['error']=repr(error)
        raise
    finally:
        if not result['passed']:
            try:
                link.command('display off')
                link.command('display recover')
            except Exception:
                pass
        result['transcript']=link.responses
        args.report.parent.mkdir(parents=True,exist_ok=True)
        args.report.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
        link.close()
    print(json.dumps({k:v for k,v in result.items() if k!='transcript'},indent=2))


if __name__=='__main__':
    main()
