const assert=require('node:assert/strict');
const {rotation,multiply}=require('./src/phone-attitude.js');
const {ratesFromMotion,ratesFromMatrices,ballFromAcceleration,effectiveRate,deflection,ballPosition,PhoneTurnSource}=require('./src/turn-coordinator.js');
const near=(a,b,tol=1e-7)=>assert.ok(Math.abs(a-b)<tol,`${a} should equal ${b}`);
function eventRate(p,q,r,angle=0){const a=angle*Math.PI/180,c=Math.cos(a),s=Math.sin(a);return {alpha:-p,beta:c*q-s*r,gamma:-s*q-c*r};}
function acceleration(bank,angle=0){const b=bank*Math.PI/180,a=angle*Math.PI/180,x=-9.81*Math.sin(b),y=9.81*Math.cos(b);return {x:Math.cos(a)*x+Math.sin(a)*y,y:-Math.sin(a)*x+Math.cos(a)*y,z:0};}
for(const screen of [0,90,180,270]){
 for(const [p,q,r]of [[0,0,3],[0,0,-3],[5,0,0],[-5,0,0],[0,10,0],[0,-10,0],[4,-7,2]]){const actual=ratesFromMotion(eventRate(p,q,r,screen),screen);near(actual.p,p);near(actual.q,q);near(actual.r,r);}
 for(const [bank,ball]of [[0,0],[15,1],[-15,-1],[7.5,0.5],[-7.5,-0.5],[30,1]])near(ballFromAcceleration(acceleration(bank,screen),screen),ball);
}
assert.equal(ratesFromMotion({alpha:null,beta:0,gamma:0}),null);
assert.equal(ballFromAcceleration({x:0,y:0,z:0}),null);assert.equal(ballFromAcceleration({x:null,y:9.81,z:0}),null);
near(deflection(effectiveRate(0,3)),20);near(deflection(effectiveRate(0,-3)),-20);near(deflection(effectiveRate(0,0)),0);near(deflection(effectiveRate(3/Math.tan(Math.PI/6),0)),20);near(deflection(30),45);
near(ballPosition(0).x,240);near(ballPosition(0).y,370);assert.ok(ballPosition(1).x>340);assert.ok(ballPosition(-1).x<140);
const upright=rotation(0,90,0),dt=0.02,rz=d=>{const a=d*Math.PI/180;return [Math.cos(a),-Math.sin(a),0,Math.sin(a),Math.cos(a),0,0,0,1];},ry=d=>{const a=d*Math.PI/180;return [Math.cos(a),0,Math.sin(a),0,1,0,-Math.sin(a),0,Math.cos(a)];},rx=d=>{const a=d*Math.PI/180;return [1,0,0,0,Math.cos(a),-Math.sin(a),0,Math.sin(a),Math.cos(a)];};
for(const [cur,p,q,r]of [[multiply(rz(-3*dt),upright),0,0,3],[multiply(ry(5*dt),upright),5,0,0],[multiply(rx(7*dt),upright),0,7,0]]){const actual=ratesFromMatrices(upright,cur,dt);near(actual.p,p);near(actual.q,q);near(actual.r,r);}
const banked=multiply(ry(30),upright);const held=ratesFromMatrices(banked,banked,dt);near(held.p,0);near(held.r,0);assert.equal(ratesFromMatrices(upright,banked,2),null);
function environment(){const events=new Map(),docEvents=new Map(),screenEvents=new Map();const env={time:100,isSecureContext:true,DeviceMotionEvent:class{},performance:{now:()=>env.time},document:{hidden:false,permissionsPolicy:{allowsFeature:()=>true},addEventListener:(k,f)=>docEvents.set(k,f),removeEventListener:k=>docEvents.delete(k)},screen:{orientation:{angle:0,addEventListener:(k,f)=>screenEvents.set(k,f),removeEventListener:k=>screenEvents.delete(k)}},addEventListener:(k,f)=>events.set(k,f),removeEventListener:k=>events.delete(k)};return {env,events,docEvents,screenEvents};}
(async()=>{
 const f=environment(),phone=new PhoneTurnSource(f.env);await phone.connect();assert.equal(phone.snapshot().valid,false);assert.equal(phone.snapshot().pending,true);
 phone.receiveMotion({rotationRate:eventRate(0,0,3),accelerationIncludingGravity:acceleration(0)});let data=phone.snapshot();assert.equal(data.valid,true);assert.equal(data.ballValid,true);near(data.r,3);near(data.ball,0);assert.equal(data.source,'phone gyro');
 // A recent gyro sample wins over orientation-derived rates.
 phone.receiveOrientation({matrix:upright,time:f.env.time});f.env.time+=20;phone.receiveOrientation({matrix:upright,time:f.env.time});near(phone.snapshot().r,3);
 f.env.time+=1600;data=phone.snapshot();assert.equal(data.valid,false);assert.equal(data.ballValid,false);
 phone.receiveOrientation({matrix:upright,time:f.env.time});f.env.time+=20;phone.receiveOrientation({matrix:multiply(rz(-0.06),upright),time:f.env.time});data=phone.snapshot();assert.equal(data.valid,true);assert.equal(data.source,'orientation-derived');near(data.r,3);assert.equal(data.ballValid,false);
 // Invalid gravity cannot masquerade as a centered, valid coordination ball.
 phone.receiveMotion({rotationRate:eventRate(0,0,0),accelerationIncludingGravity:{x:0,y:0,z:0}});assert.equal(phone.snapshot().ballValid,false);
 f.env.document.hidden=true;assert.equal(phone.snapshot().valid,false);f.docEvents.get('visibilitychange')();f.env.document.hidden=false;assert.equal(phone.snapshot().valid,false);
 phone.disconnect();assert.equal(f.events.size,0);assert.equal(f.screenEvents.size,0);assert.equal(f.docEvents.size,0);
 for(const [setup,status]of [[e=>e.isSecureContext=false,'secure'],[e=>e.document.permissionsPolicy.allowsFeature=()=>false,'blocked'],[e=>e.DeviceMotionEvent.requestPermission=async()=> 'denied','denied']]){const x=environment();setup(x.env);const p=new PhoneTurnSource(x.env);await p.connect();assert.equal(p.status,status);assert.equal(x.events.size,0);}
 const x=environment();let finish;x.env.DeviceMotionEvent.requestPermission=()=>new Promise(resolve=>finish=resolve);const canceled=new PhoneTurnSource(x.env),connection=canceled.connect();canceled.disconnect();finish('granted');await connection;assert.equal(canceled.connected,false);assert.equal(x.events.size,0);
 console.log('Turn coordinator checks passed: gyro axes in all four screen orientations; isolated roll/yaw/pitch; held bank returns to zero rate; standard-rate marks; ball direction and freefall rejection; orientation fallback, gyro preference, stale data, visibility and permissions.');
})().catch(error=>{console.error(error);process.exit(1);});
