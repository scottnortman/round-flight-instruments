const assert=require('node:assert/strict');
const {rotation,tilt,multiply,transpose,availability,PhoneAttitudeSource}=require('./src/phone-attitude.js');
const near=(a,b)=>assert.ok(Math.abs(a-b)<1e-8,`${a} should equal ${b}`);
const I=rotation(0,0,0);
near(tilt(I,I).pitchDeg,0);near(tilt(I,I).rollDeg,0);
near(tilt(I,rotation(0,10,0)).pitchDeg,10);near(tilt(I,rotation(20,0,0)).rollDeg,-20);
near(tilt(I,rotation(0,-10,0)).pitchDeg,-10);near(tilt(I,rotation(-30,0,0)).rollDeg,30);
near(tilt(I,rotation(90,0,0,90)).pitchDeg,0);near(tilt(I,rotation(90,0,0,90)).rollDeg,0);
near(tilt(rotation(359,90,0),rotation(1,90,0)).pitchDeg,0);near(tilt(rotation(359,90,0),rotation(1,90,0)).rollDeg,0);near(tilt(rotation(359,90,0),rotation(1,90,0)).yawDeg,-2);
const T=[0,1,0,0,0,-1,-1,0,0],base=rotation(127,65,-12);
const ry=a=>{a*=Math.PI/180;return [Math.cos(a),0,Math.sin(a),0,1,0,-Math.sin(a),0,Math.cos(a)];};
const rx=a=>{a*=Math.PI/180;return [1,0,0,0,Math.cos(a),-Math.sin(a),0,Math.sin(a),Math.cos(a)];};
const current=multiply(base,multiply(T,multiply(multiply(ry(15),rx(-35)),transpose(T))));
near(tilt(base,current).pitchDeg,15);near(tilt(base,current).rollDeg,-35);
// Independent physical fixtures: screen vertical, front facing the observer.
// Known W3C event angles reproduce the user's yaw/bank exchange exactly.
const upright=rotation(0,90,0);
for(const [name,event,expected]of [
  ['nose up',[0,100,0],[10,0,0]],
  ['nose down',[0,80,0],[-10,0,0]],
  ['right bank',[270,60,90],[0,30,0]],
  ['left bank',[90,60,-90],[0,-30,0]],
  ['right yaw',[330,90,0],[0,0,30]],
  ['left yaw',[30,90,0],[0,0,-30]]
]){const actual=tilt(upright,rotation(...event));try{near(actual.pitchDeg,expected[0]);near(actual.rollDeg,expected[1]);near(actual.yawDeg,expected[2]);}catch(e){throw new Error(name+': '+e.message);}}
// Generate browser inputs from aircraft poses in world coordinates, rather
// than the implementation's screen-to-aircraft matrix. World X is right,
// world Y is forward, and world Z is up at the initial level panel pose.
const rz=a=>{a*=Math.PI/180;return [Math.cos(a),-Math.sin(a),0,Math.sin(a),Math.cos(a),0,0,0,1];};
function browserAngles(m){const beta=Math.asin(Math.max(-1,Math.min(1,m[7]))),regular=Math.abs(Math.cos(beta))>1e-8;return {alpha:(regular?Math.atan2(-m[1],m[4]):Math.atan2(m[3],m[0]))*180/Math.PI,beta:beta*180/Math.PI,gamma:regular?Math.atan2(-m[6],m[8])*180/Math.PI:0};}
function panelEvent(p,r,y=0,heading=0,screen=0){const worldAircraft=multiply(rz(heading),multiply([0,1,0,1,0,0,0,0,-1],multiply(rz(y),multiply(ry(p),rx(r)))));const a=worldAircraft;const phone=[a[1],-a[2],-a[0],a[4],-a[5],-a[3],a[7],-a[8],-a[6]];return browserAngles(multiply(phone,rz(screen)));}
function eventMatrix(e,screen=0){return rotation(e.alpha,e.beta,e.gamma,screen);}
for(const screen of [0,90,180,270])for(const heading of [0,37,359]){
  const reference=eventMatrix(panelEvent(0,0,0,heading,screen),screen);
  for(const [p,r,y]of [[10,0,0],[-10,0,0],[0,30,0],[0,-30,0],[0,0,30],[0,0,-30],[12,-25,40],[-12,25,-40],[0,0,179],[0,0,-179]]){
    const actual=tilt(reference,eventMatrix(panelEvent(p,r,y,heading,screen),screen));near(actual.pitchDeg,p);near(actual.rollDeg,r);near(actual.yawDeg,y);
  }
}
function environment(){
  const events=new Map(),screenEvents=new Map(),docEvents=new Map();
  const env={time:100,isSecureContext:true,DeviceOrientationEvent:class{},performance:{now:()=>env.time},screen:{orientation:{angle:0,addEventListener:(k,f)=>screenEvents.set(k,f),removeEventListener:k=>screenEvents.delete(k)}},document:{hidden:false,permissionsPolicy:{allowsFeature:()=>true},addEventListener:(k,f)=>docEvents.set(k,f),removeEventListener:k=>docEvents.delete(k)},addEventListener:(k,f)=>events.set(k,f),removeEventListener:k=>events.delete(k),setInterval:f=>{env.tick=f;return 1;},clearInterval:()=>{env.tick=null;}};
  return {env,events,screenEvents,docEvents};
}
(async()=>{
  const f=environment(),samples=[],validity=[],statuses=[];
  const phone=new PhoneAttitudeSource(f.env,{sample:s=>samples.push(s),validity:v=>validity.push(v),status:s=>statuses.push(s)});
  await phone.connect();assert.equal(phone.status,'waiting');assert.equal(phone.connected,true);assert.equal(availability(phone.status,phone.valid),'pending');
  phone.receive({alpha:null,beta:null,gamma:null});assert.equal(phone.latest,null);assert.equal(phone.diagnostics().events,1);assert.equal(phone.diagnostics().readings,0);
  phone.receive(panelEvent(0,0));assert.equal(phone.status,'ready');assert.equal(phone.valid,false);assert.equal(availability(phone.status,phone.valid),'pending');assert.equal(phone.diagnostics().readings,1);assert.equal(phone.diagnostics().calibrated,false);
  assert.equal(phone.calibrate(),true);assert.equal(phone.valid,true);near(samples.at(-1).pitchDeg,0);assert.equal(availability(phone.status,phone.valid),'valid');assert.equal(availability(phone.status,phone.valid,true),'failed');assert.equal(phone.diagnostics().calibrated,true);
  for(let i=0;i<100;i++){f.env.time+=16;phone.receive(panelEvent(10,20));}
  assert.ok(Math.abs(samples.at(-1).pitchDeg-10)<0.01);assert.ok(Math.abs(samples.at(-1).rollDeg-20)<0.01);
  f.env.time+=3000;phone.check();assert.equal(phone.status,'stale');assert.equal(phone.valid,false);assert.equal(phone.calibrate(),false);assert.equal(availability(phone.status,phone.valid),'failed');
  f.env.time+=16;phone.receive(panelEvent(10,20));assert.equal(phone.status,'live');
  for(let i=0;i<100;i++){f.env.time+=16;phone.receive(panelEvent(0,0,30));}assert.ok(Math.abs(samples.at(-1).rollDeg)<0.01);assert.ok(Math.abs(samples.at(-1).pitchDeg)<0.01);assert.ok(Math.abs(samples.at(-1).yawDeg-30)<0.01);
  for(const yaw of [179,-179])for(let i=0;i<100;i++){f.env.time+=16;phone.receive(panelEvent(0,0,yaw));}assert.ok(Math.abs(samples.at(-1).yawDeg+179)<0.01);assert.ok(Math.abs(samples.at(-1).rollDeg)<0.01);
  phone.receive(panelEvent(80,0));assert.equal(phone.status,'range');assert.equal(phone.valid,false);
  f.screenEvents.get('change')();assert.equal(phone.latest,null);assert.equal(phone.reference,null);assert.equal(phone.calibrate(),false);
  phone.receive({alpha:0,beta:0,gamma:0});assert.equal(phone.calibrate(),true);
  f.env.document.hidden=true;f.docEvents.get('visibilitychange')();assert.equal(phone.status,'paused');assert.equal(phone.latest,null);
  phone.disconnect();assert.equal(f.events.size,0);assert.equal(f.env.tick,null);
  for(const [setup,status]of [[e=>{e.isSecureContext=false;},'secure'],[e=>{delete e.DeviceOrientationEvent;},'unsupported'],[e=>{e.document.permissionsPolicy.allowsFeature=()=>false;},'blocked'],[e=>{e.DeviceOrientationEvent.requestPermission=async()=> 'denied';},'denied']]){
    const x=environment();setup(x.env);const p=new PhoneAttitudeSource(x.env,{});await p.connect();assert.equal(p.status,status);assert.equal(p.connected,false);assert.equal(p.pending,false);assert.equal(x.events.size,0);
  }
  const absent=environment(),p=new PhoneAttitudeSource(absent.env,{});await p.connect();absent.env.time+=7000;p.check();assert.equal(p.status,'unavailable');p.disconnect();
  const unexpected=environment();unexpected.env.DeviceOrientationEvent.requestPermission=async()=>{throw new TypeError('API failure');};const broken=new PhoneAttitudeSource(unexpected.env,{});await broken.connect();assert.equal(broken.status,'error');assert.equal(broken.diagnostics().error,'TypeError');assert.equal(broken.connected,false);
  console.log('Passed: six upright single-axis regression fixtures; 120 aircraft poses across portrait, both landscapes, inverted screen and three headings; yaw/roll separation through filtering; calibration, range, stale data, visibility and permission lifecycle checks.');
})().catch(e=>{console.error(e);process.exit(1);});
