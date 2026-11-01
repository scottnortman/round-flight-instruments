const assert=require('node:assert/strict');
const {rotation,multiply}=require('./src/phone-attitude.js');
const {normalize,delta,format,cardLabel,cardRotation,RelativeHeadingSource}=require('./src/heading.js');
const near=(a,b,tolerance=1e-7)=>assert.ok(Math.abs(a-b)<tolerance,`${a} should equal ${b}`);
near(normalize(-1),359);near(normalize(360),0);near(normalize(721),1);assert.equal(normalize(null),null);assert.equal(normalize(NaN),null);
near(delta(1,359),2);near(delta(359,1),-2);assert.equal(format(0),'360°');assert.equal(format(90),'090°');assert.equal(format(359),'359°');assert.equal(format(NaN),'— — —');
assert.deepEqual([0,30,60,90,120,150,180,210,240,270,300,330].map(cardLabel),['N','3','6','E','12','15','S','21','24','W','30','33']);
for(const h of [0,45,90,180,270,359])near(normalize(h+cardRotation(h)),0); // heading mark reaches fixed top index
const RAD=Math.PI/180,rx=d=>{const a=d*RAD;return [1,0,0,0,Math.cos(a),-Math.sin(a),0,Math.sin(a),Math.cos(a)];},ry=d=>{const a=d*RAD;return [Math.cos(a),0,Math.sin(a),0,1,0,-Math.sin(a),0,Math.cos(a)];},rz=d=>{const a=d*RAD;return [Math.cos(a),-Math.sin(a),0,Math.sin(a),Math.cos(a),0,0,0,1];};
// Ground truth: world X right, Y forward, Z up. Form the phone pose from
// an aircraft pose: phone-right=aircraft-right, top=-down, out=-forward.
function panel(p,r,y,heading=0){const a=multiply(rz(heading),multiply([0,1,0,1,0,0,0,0,-1],multiply(rz(y),multiply(ry(p),rx(r)))));return [a[1],-a[2],-a[0],a[4],-a[5],-a[3],a[7],-a[8],-a[6]];}
function fixture(initial=0){const env={time:100,performance:{now:()=>env.time},document:{hidden:false}},source=new RelativeHeadingSource(env);source.start(initial);source.waitForReadings();const feed=matrix=>{env.time+=20;source.receive({matrix,time:env.time,complete:true});};return {env,source,feed};}
for(const heading of [0,37,359])for(const [pitch,roll,yaw]of [[0,0,90],[0,0,-90],[10,0,0],[-10,0,0],[0,30,0],[0,-30,0],[12,-25,40],[70,0,30]]){
 const {source,feed}=fixture(45);feed(panel(0,0,0,heading));assert.equal(source.snapshot().pending,true);assert.equal(source.calibrate(),true);
 for(let i=0;i<100;i++)feed(panel(pitch,roll,yaw,heading));assert.equal(source.snapshot().valid,true);near(delta(source.snapshot().headingDeg,normalize(45+yaw)),0);
}
{
 const {source,feed,env}=fixture();feed(rotation(0,90,0));source.calibrate();
 // Reproduce browser's upright yaw fixture: positive right turn => 090.
 for(let i=0;i<100;i++)feed(rotation(270,90,0));near(source.snapshot().headingDeg,90);
 assert.equal(source.setIndicatedHeading(200),true);near(source.snapshot().headingDeg,200);
 for(let i=0;i<100;i++)feed(rotation(260,90,0));near(source.snapshot().headingDeg,210);
 // Re-leveling changes the zero pose while preserving indicated heading.
 source.calibrate();near(source.snapshot().headingDeg,210);for(let i=0;i<100;i++)feed(rotation(250,90,0));near(source.snapshot().headingDeg,220);
 env.time+=3000;assert.equal(source.snapshot().valid,false);assert.equal(source.snapshot().status,'stale');assert.equal(source.calibrate(),false);
 feed(rotation(250,90,0));assert.equal(source.snapshot().valid,true);
 source.invalidateReference('rotated');assert.equal(source.snapshot().valid,false);assert.equal(source.snapshot().pending,true);feed(rotation(250,90,0));assert.equal(source.snapshot().status,'ready');source.calibrate();near(source.snapshot().headingDeg,220);
 env.document.hidden=true;assert.equal(source.snapshot().valid,false);source.invalidateReference('paused');env.document.hidden=false;feed(rotation(250,90,0));assert.equal(source.snapshot().valid,false);source.calibrate();assert.equal(source.snapshot().valid,true);
 source.receive({matrix:rotation(0,90,0),time:env.time,complete:false});assert.equal(source.snapshot().valid,false);assert.equal(source.snapshot().status,'incomplete');
 source.stop();assert.equal(source.snapshot().valid,false);assert.equal(source.snapshot().pending,false);
}
{
 const {source,feed}=fixture();feed(panel(0,0,0));source.calibrate();
 for(let y=0;y<=720;y+=2){feed(panel(0,0,y));assert.ok(Math.abs(delta(source.snapshot().headingDeg,normalize(y)))<10,'Full revolutions must remain continuous');}
 for(let i=0;i<100;i++)feed(panel(0,0,359));const before=source.snapshot().headingDeg;feed(panel(0,0,1));assert.ok(Math.abs(delta(source.snapshot().headingDeg,before))<2.01,'North crossing must take the short path');
 for(let i=0;i<100;i++)feed(panel(0,0,1));near(delta(source.snapshot().headingDeg,1),0);
 feed(panel(85,0,0));assert.equal(source.snapshot().valid,false);assert.equal(source.snapshot().status,'vertical');
}
{
 const {source,env}=fixture(123);env.time+=7000;assert.equal(source.snapshot().status,'unavailable');assert.equal(source.snapshot().pending,false);assert.equal(source.setIndicatedHeading(NaN),false);near(source.snapshot().headingDeg,123);
 source.receive({matrix:rotation(0,90,0),time:NaN,complete:true});assert.equal(source.snapshot().valid,false);
}
{const env={time:0,performance:{now:()=>env.time},document:{hidden:false}},pending=new RelativeHeadingSource(env);pending.start();env.time=10000;assert.equal(pending.snapshot().pending,true);pending.waitForReadings();assert.equal(pending.snapshot().status,'waiting');env.time+=7000;assert.equal(pending.snapshot().status,'unavailable');}
console.log('Heading checks passed: compass-card labels/direction, 24 independent 3D axis fixtures, right/left turns, pitch/roll separation, multiple revolutions and north crossing, reference adjustment/re-level continuity, stale/incomplete/vertical data, visibility and reset handling.');
