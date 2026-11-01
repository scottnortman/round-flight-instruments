"""Plot captured motion and a separately measured stationary baseline, uncorrected."""
import argparse
from collections import Counter
import csv
from datetime import datetime
import json
from pathlib import Path
from zoneinfo import ZoneInfo
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--capture', type=Path, required=True)
parser.add_argument('--stationary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True, help='output path without extension')
parser.add_argument('--stationarity-confirmed', action='store_true')
parser.add_argument('--prior-saturation', action='store_true')
args = parser.parse_args()
capture = json.loads(args.capture.read_text(encoding='utf-8'))
baseline = json.loads(args.stationary.read_text(encoding='utf-8'))
events = capture['events']
valid = [e for e in events if 'sample' in e]
if not capture['completed'] or not valid:
    raise ValueError('A completed capture with valid samples is required')
t = np.array([e['t_s'] for e in events])

def series(key):
    return np.array([float(e.get('sample', {}).get(key, 'nan').replace('none', 'nan')) for e in events])

colors = ['#2563a6', '#d97706', '#16836f']
plt.rcParams.update({'font.family': 'DejaVu Sans', 'font.size': 10,
                     'axes.spines.top': False, 'axes.spines.right': False,
                     'axes.titleweight': 'bold', 'axes.titlesize': 12,
                     'figure.facecolor': '#f8fafc', 'axes.facecolor': 'white',
                     'grid.color': '#dce3eb', 'grid.alpha': 0.65})
fig, axes = plt.subplots(2, 2, figsize=(14, 9.5))
fig.subplots_adjust(left=0.075, right=0.97, bottom=0.14, top=0.84, hspace=0.42, wspace=0.25)
local_time = datetime.fromisoformat(capture['timestamp_utc']).astimezone(ZoneInfo('America/New_York'))
fig.suptitle('Onboard IMU: motion response and stationary gyro offset', x=0.075, y=0.96,
             ha='left', fontsize=20, fontweight='bold', color='#152b45')
fig.text(0.075, 0.905,
         f'QMI8658C  |  {local_time:%b %d, %Y · %I:%M %p %Z}  |  '
         f'{len(valid)} valid readings / {len(events)-len(valid)} rejected  |  sensor-native axes',
         fontsize=11, color='#475569')

ax = axes[0, 0]
for name, color in zip('xyz', colors):
    ax.plot(t, series(f'a{name}_g'), label=name.upper(), color=color, lw=1.5)
ax.plot(t, series('norm_g'), label='Magnitude', color='#64748b', lw=1, ls=':')
ax.set(title='Acceleration changes with board orientation', ylabel='Acceleration (g)', xlabel='Host receipt time (s)')
ax.legend(ncols=4, fontsize=9, loc='lower left')

stationary = np.array([[float(s[f'g{axis}_dps']) for axis in 'xyz'] for s in baseline['samples']])
means = stationary.mean(axis=0)
stddev = stationary.std(axis=0, ddof=1)
ax = axes[0, 1]
for name, color in zip('xyz', colors):
    ax.plot(t, series(f'g{name}_dps'), label=name.upper(), color=color, lw=1.3)
ax.axhline(means[1], color=colors[1], ls='--', lw=1, alpha=0.7)
ax.set(title='Raw angular rate — no offset removed', ylabel='Angular rate (degrees/s)', xlabel='Host receipt time (s)')
ax.legend(ncols=3, fontsize=9, loc='lower left')

ax = axes[1, 0]
pitch = series('tilt_pitch_deg')
roll = series('tilt_roll_deg')
roll_breaks = np.insert(np.abs(np.diff(roll)) > 180, 0, False)
roll_plot = roll.copy()
roll_plot[roll_breaks] = np.nan
ax.plot(t, pitch, color='#7c3aed', label='Pitch', lw=1.5)
ax.plot(t, roll_plot, color='#0891b2', label='Roll (wrapped ±180°)', lw=1.3)
ax.scatter(t, roll, color='#0891b2', s=7)
ax.set(title='Accelerometer-only tilt — not fused attitude', ylabel='Tilt (degrees)', xlabel='Host receipt time (s)', ylim=(-190,190))
ax.legend(fontsize=9, loc='lower left')

ax = axes[1, 1]
ax.axhspan(-10, 10, color='#e2e8f0', zorder=0, label='Datasheet initial offset ±10°/s')
ax.bar(['X', 'Y', 'Z'], means, color=colors, width=0.55, zorder=3)
ax.errorbar(range(3), means, yerr=stddev, fmt='none', ecolor='#1e293b', capsize=5, lw=1, zorder=4)
for i, mean in enumerate(means):
    ax.annotate(f'{mean:+.2f}°/s', (i, mean), xytext=(0, 9 if mean >= 0 else -17),
                textcoords='offset points', ha='center', fontweight='bold', fontsize=11)
ax.axhline(0, color='#94a3b8', lw=0.8)
ax.set(title=('Earlier baseline: user confirmed board stationary' if args.stationarity_confirmed
              else 'Reference capture: stationarity unconfirmed'), ylabel='Mean raw gyro rate (degrees/s)', ylim=(-14,40))
ax.legend(fontsize=9, loc='upper left')
ax.text(0.97,0.04,'Error bars: ±1 standard deviation',transform=ax.transAxes,ha='right',fontsize=8,color='#475569')

for ax in axes.flat:
    ax.grid(axis='y')
    ax.set_axisbelow(True)
for ax in (axes[0,0],axes[0,1],axes[1,0]):
    ax.set_xlim(0, max(15,t[-1]))
    for i,e in enumerate(events):
        if 'error' in e:
            ax.axvspan(e['t_s']-0.05,e['t_s']+0.05,color='#dc2626',alpha=0.18)

fig.text(0.075,0.072,
         f'Key finding: stationary Y gyro = {means[1]:+.2f}°/s. Motion response is present; calibration/accuracy remains unresolved.',
         fontsize=11,color='#9a3412',fontweight='bold')
fig.text(0.075,0.037,
         'No bias correction or smoothing applied. Roll lines break at angle wrap; tilt may be absent under acceleration.\n'
         'Time is host receipt time (~8 observations/s), not the sensor ODR.' +
         (' The first, separate attempt stopped on saturation.' if args.prior_saturation else ''),
         fontsize=9,color='#475569',linespacing=1.6)
args.output.parent.mkdir(parents=True, exist_ok=True)
for suffix in ('png','svg','pdf'):
    fig.savefig(args.output.with_suffix('.'+suffix), dpi=160)

keys = ['ax_g','ay_g','az_g','gx_dps','gy_dps','gz_dps','temp_c','norm_g','tilt_pitch_deg','tilt_roll_deg','age_ms','counter','sample']
with args.output.with_suffix('.csv').open('w',newline='',encoding='utf-8') as stream:
    writer=csv.DictWriter(stream,fieldnames=['t_s','response_ms','error']+keys)
    writer.writeheader()
    for e in events:
        writer.writerow({'t_s':e['t_s'],'response_ms':e['response_ms'],'error':e.get('error',''),
                         **{k:e.get('sample',{}).get(k,'') for k in keys}})
summary = {'completed':capture['completed'], 'valid_readings':len(valid),
           'rejected_readings':dict(Counter(e['error'] for e in events if 'error' in e)),
           'duration_s':float(t[-1]),
           'acceleration_ranges_g':{a:[float(np.nanmin(series('a'+a+'_g'))),float(np.nanmax(series('a'+a+'_g')))] for a in 'xyz'},
           'gyro_ranges_dps':{a:[float(np.nanmin(series('g'+a+'_dps'))),float(np.nanmax(series('g'+a+'_dps')))] for a in 'xyz'},
           'stationary_mean_gyro_dps':dict(zip('xyz',means.tolist())),
           'stationary_stddev_gyro_dps':dict(zip('xyz',stddev.tolist())),
           'stationarity_confirmed_by_user':args.stationarity_confirmed,
           'initial_attempt_saturated':args.prior_saturation,
           'capture_file':str(args.capture),'stationary_file':str(args.stationary)}
args.output.with_suffix('.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
print(json.dumps(summary,indent=2))
