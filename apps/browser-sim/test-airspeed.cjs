const assert=require('node:assert/strict');
const {profile,validateProfile,angleAt,pointAt,arcPath,inspect}=require('./src/airspeed.js');
const near=(a,b)=>assert.ok(Math.abs(a-b)<1e-9,`${a} should equal ${b}`);
// C172S figure 2-2 KIAS values. No CAS conversion or GPS speed substitution.
assert.deepEqual(profile.white,[40,85]);assert.deepEqual(profile.green,[48,129]);assert.deepEqual(profile.yellow,[129,163]);assert.equal(profile.red,163);assert.equal(profile.unit,'KIAS');
near(angleAt(0),0);near(angleAt(40),60);near(angleAt(60),105);near(angleAt(85),171.25);near(angleAt(120),242);near(angleAt(129),256.4);near(angleAt(163),302.45);near(angleAt(200),342);
let previous=-Infinity;for(let value=0;value<=200;value+=0.25){const angle=angleAt(value);assert.ok(angle>previous,'Increasing IAS must advance the needle clockwise');previous=angle;const p=pointAt(value,198);near(Math.hypot(p.x-240,p.y-240),198);assert.equal(inspect(value).valid,true);}
for(const value of [NaN,Infinity,-1,201,null,'85'])assert.equal(inspect(value).valid,false);
assert.equal(inspect(0).valid,true,'Zero airspeed is valid, unlike a missing input');assert.equal(inspect(85,false).valid,false);
for(const [value,zone]of [[39.9,'below'],[40,'white'],[47.9,'white'],[48,'white-green'],[85,'white-green'],[85.1,'green'],[128.9,'green'],[129,'yellow'],[162.9,'yellow'],[163,'red'],[163.1,'over']])assert.equal(inspect(value).zone,zone,`${value} KIAS boundary`);
// Each arc uses the same mapping as the pointer and scale graduations.
for(const [range,r]of [[profile.white,203],[profile.green,216],[profile.yellow,216]]){const start=pointAt(range[0],r),end=pointAt(range[1],r),path=arcPath(...range,r);assert.ok(path.startsWith(`M${start.x} ${start.y}`));assert.ok(path.endsWith(`${end.x} ${end.y}`));}
assert.throws(()=>arcPath(85,40,203));assert.throws(()=>validateProfile({...profile,red:160}));assert.throws(()=>validateProfile({...profile,scale:[[0,0],[100,150],[90,170],[200,342]]}));assert.ok(Object.isFrozen(profile)&&Object.isFrozen(profile.white)&&Object.isFrozen(profile.scale[0]));
console.log('Airspeed checks passed: C172S KIAS markings, arc overlap/endpoints, continuous clockwise nonlinear scale, zero-vs-invalid data, exact limit boundaries, overspeed indication and profile validation.');
