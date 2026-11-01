/* Relative heading bench model. Conventional compass card, not a magnetic
   compass. Uses the established upright-panel mounting transform from the
   attitude adapter. No magnetometer heading, GPS, recording or network calls. */
(function(global){
  const math=typeof module!=='undefined'&&module.exports?require('./phone-attitude.js'):global.PhoneAttitude;
  const normalize=value=>Number.isFinite(value)?((value%360)+360)%360:null;
  const delta=(a,b)=>{const difference=normalize(a-b);return difference===null?null:difference>=180?difference-360:difference;};
  const format=value=>{const h=normalize(value);return h===null?'— — —':String(Math.round(h)%360||360).padStart(3,'0')+'°';};
  const cardLabel=angle=>({0:'N',90:'E',180:'S',270:'W'}[normalize(angle)]??String(normalize(angle)/10));
  const cardRotation=heading=>{const h=normalize(heading);return h===null?null:-h;};
  class RelativeHeadingSource {
    constructor(env){this.env=env;this.active=false;this.baseHeading=0;this.lastHeading=0;this.reference=null;this.latest=null;this.valid=false;this.status='off';this.lastYaw=0;this.filterTime=null;this.started=0;}
    now(){return this.env.performance.now();}
    start(heading=0){this.active=true;this.baseHeading=normalize(heading)??0;this.lastHeading=this.baseHeading;this.reference=null;this.latest=null;this.valid=false;this.status='permission';this.lastYaw=0;this.filterTime=null;this.started=this.now();}
    waitForReadings(){if(this.active&&!this.reference&&!this.latest){this.status='waiting';this.started=this.now();}}
    invalidateReference(reason){if(!this.active)return;this.baseHeading=this.lastHeading;this.reference=null;this.latest=null;this.valid=false;this.status=reason;this.filterTime=null;this.lastYaw=0;this.started=this.now();}
    receive(reading){
      if(!this.active)return;
      if(this.env.document.hidden){this.invalidateReference('paused');return;}
      if(!Number.isFinite(reading.time)||reading.complete===false||!Array.isArray(reading.matrix)||reading.matrix.length!==9||!reading.matrix.every(Number.isFinite)){this.latest=null;this.valid=false;this.status='incomplete';return;}
      this.latest={matrix:reading.matrix.slice(),time:reading.time};
      if(!this.reference){this.valid=false;this.status='ready';return;}
      const pose=math.tilt(this.reference,reading.matrix);
      // Heading loses its horizontal reference near vertical nose direction.
      if(Math.abs(pose.pitchDeg)>80){this.valid=false;this.status='vertical';return;}
      this.lastYaw=pose.yawDeg;
      const target=normalize(this.baseHeading+pose.yawDeg),dt=this.filterTime===null?1:Math.max(0,Math.min(0.25,(reading.time-this.filterTime)/1000));
      const weight=1-Math.exp(-dt/0.09);
      this.lastHeading=normalize(this.lastHeading+weight*delta(target,this.lastHeading));this.filterTime=reading.time;this.valid=true;this.status='live';
    }
    calibrate(){
      if(!this.active||!this.latest||this.now()-this.latest.time>2500||this.env.document.hidden)return false;
      this.reference=this.latest.matrix.slice();this.baseHeading=this.lastHeading;this.lastYaw=0;this.filterTime=this.now();this.valid=true;this.status='live';return true;
    }
    setIndicatedHeading(value){
      const h=normalize(value);if(h===null)return false;
      if(this.snapshot().valid)this.baseHeading=normalize(h-this.lastYaw);
      else{this.baseHeading=h;this.reference=null;if(this.active&&this.latest&&this.now()-this.latest.time<=2500)this.status='ready';}
      this.lastHeading=h;this.filterTime=this.now();return true;
    }
    snapshot(){
      const visible=this.active&&!this.env.document.hidden,fresh=!!this.latest&&this.now()-this.latest.time<=2500;
      let status=this.status;if(this.active&&this.env.document.hidden)status='paused';else if(this.active&&this.latest&&!fresh)status='stale';else if(status==='waiting'&&this.now()-this.started>6000)status='unavailable';
      return {headingDeg:this.lastHeading,valid:visible&&fresh&&this.valid,pending:visible&&['permission','waiting','ready','rotated','paused'].includes(status),status,source:'phone-relative',referenced:!!this.reference};
    }
    stop(){this.active=false;this.reference=null;this.latest=null;this.valid=false;this.status='off';}
  }
  const api={normalize,delta,format,cardLabel,cardRotation,RelativeHeadingSource};
  if(typeof module!=='undefined'&&module.exports)module.exports=api;else global.Heading=api;
})(typeof window!=='undefined'?window:globalThis);
