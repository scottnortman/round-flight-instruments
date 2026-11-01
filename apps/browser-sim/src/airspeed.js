/* Single-engine IAS bench model. Limits: Cessna 172S POH, fig.2-2.
   Dial conventions: FAA PHAK fig.8-8 and United UI8000-B445 p.2 (8025 B.477).
   Representative nonlinear scale, not a measured copy of a specific instrument.
   Aircraft limits, dial geometry and input validity are separate from rendering. */
(function(global){
  const profile=Object.freeze({id:'c172s-example',label:'C172S POH example',shortLabel:'C172S REF',unit:'KIAS',maxKt:200,
    white:Object.freeze([40,85]),green:Object.freeze([48,129]),yellow:Object.freeze([129,163]),red:163,
    scale:Object.freeze([[0,0],[20,10],[40,60],[60,105],[80,160],[100,205],[120,242],[140,274],[160,299],[180,322],[200,342]].map(Object.freeze))});
  function validateProfile(p){
    if(!p||p.unit!=='KIAS'||!Array.isArray(p.scale)||p.scale.length<2)throw new Error('Invalid airspeed profile');
    for(let i=0;i<p.scale.length;i++){const [v,a]=p.scale[i];if(!Number.isFinite(v)||!Number.isFinite(a)||i&&(v<=p.scale[i-1][0]||a<=p.scale[i-1][1]))throw new Error('Airspeed scale must increase monotonically');}
    if(p.scale[0][0]!==0||p.scale.at(-1)[0]!==p.maxKt)throw new Error('Airspeed scale endpoints do not match its range');
    for(const [low,high]of [p.white,p.green,p.yellow])if(!(Number.isFinite(low)&&Number.isFinite(high)&&low>=0&&low<high&&high<=p.maxKt))throw new Error('Invalid airspeed arc');
    if(p.green[1]!==p.yellow[0]||p.yellow[1]!==p.red||p.white[1]>p.red)throw new Error('Inconsistent airspeed limits');
    return true;
  }
  validateProfile(profile);
  function angleAt(kt,p=profile){
    if(!Number.isFinite(kt)||kt<0||kt>p.maxKt)return null;
    for(let i=1;i<p.scale.length;i++){const [v1,a1]=p.scale[i],[v0,a0]=p.scale[i-1];if(kt<=v1)return a0+(a1-a0)*(kt-v0)/(v1-v0);}
    return null;
  }
  function pointAt(kt,r,p=profile){const a=angleAt(kt,p);if(a===null)return null;return {x:240+r*Math.sin(a*Math.PI/180),y:240-r*Math.cos(a*Math.PI/180)};}
  function arcPath(low,high,r,p=profile){const a=pointAt(low,r,p),b=pointAt(high,r,p);if(!a||!b||low>=high)throw new Error('Invalid airspeed arc endpoints');return `M${a.x} ${a.y} A${r} ${r} 0 ${angleAt(high,p)-angleAt(low,p)>180?1:0} 1 ${b.x} ${b.y}`;}
  function inspect(kt,valid=true,p=profile){
    const angle=angleAt(kt,p);if(!valid||angle===null)return {valid:false,angle:null,label:'Airspeed data unavailable',zone:'invalid'};
    const overlap=kt>=p.green[0]&&kt<=p.white[1];
    const zone=kt>p.red?'over':kt===p.red?'red':kt>=p.yellow[0]?'yellow':kt>p.white[1]?'green':overlap?'white-green':kt>=p.white[0]?'white':'below';
    const labels={over:'Above red line',red:'At red line',yellow:'Yellow arc · caution range',green:'Green arc · normal range','white-green':'White + green arcs',white:'White arc · full-flap range',below:'Below white arc'};
    return {valid:true,angle,label:labels[zone],zone};
  }
  const api={profile,validateProfile,angleAt,pointAt,arcPath,inspect};
  if(typeof module!=='undefined'&&module.exports)module.exports=api;else global.Airspeed=api;
})(typeof window!=='undefined'?window:globalThis);
