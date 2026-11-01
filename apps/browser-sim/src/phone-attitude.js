/* Local phone-attitude emulator. DeviceOrientation uses Z-X-Y device angles,
   not aircraft roll/pitch/yaw. Reference: https://www.w3.org/TR/orientation-event/
   No data is recorded or transmitted. */
(function(global){
  const support=typeof module!=='undefined'&&module.exports?require('./device-support.js'):global.DeviceSupport;
  const RAD=Math.PI/180, DEG=180/Math.PI;
  function multiply(a,b){const out=Array(9).fill(0);for(let r=0;r<3;r++)for(let c=0;c<3;c++)for(let k=0;k<3;k++)out[r*3+c]+=a[r*3+k]*b[k*3+c];return out;}
  function transpose(m){return [m[0],m[3],m[6],m[1],m[4],m[7],m[2],m[5],m[8]];}
  function rotation(alpha,beta,gamma,screenAngle=0){
    const a=alpha*RAD,b=beta*RAD,g=gamma*RAD,s=-screenAngle*RAD;
    const ca=Math.cos(a),sa=Math.sin(a),cb=Math.cos(b),sb=Math.sin(b),cg=Math.cos(g),sg=Math.sin(g);
    const device=[ca*cg-sa*sb*sg,-cb*sa,cg*sa*sb+ca*sg,cg*sa+ca*sb*sg,ca*cb,sa*sg-ca*cg*sb,-cb*sg,sb,cb*cg];
    return multiply(device,[Math.cos(s),-Math.sin(s),0,Math.sin(s),Math.cos(s),0,0,0,1]);
  }
  function tilt(reference,current){
    // Upright instrument panel: aircraft forward goes INTO the screen,
    // right follows screen-right, and down points toward the screen bottom.
    // The former face-up-phone mount used screen-top as forward, exchanging
    // upright yaw and bank. Columns are aircraft axes in screen coordinates.
    const screenToAircraft=[0,1,0,0,0,-1,-1,0,0];
    const relative=multiply(transpose(reference),current);
    const aircraft=multiply(transpose(screenToAircraft),multiply(relative,screenToAircraft));
    return {pitchDeg:Math.atan2(-aircraft[6],Math.hypot(aircraft[0],aircraft[3]))*DEG,rollDeg:Math.atan2(aircraft[7],aircraft[8])*DEG,yawDeg:Math.atan2(aircraft[3],aircraft[0])*DEG};
  }
  function angleDelta(a,b){return ((a-b+540)%360)-180;}
  function availability(status,valid,failed=false){return failed?'failed':valid?'valid':['permission','waiting','ready','paused','rotated'].includes(status)?'pending':'failed';}
  class PhoneAttitudeSource {
    constructor(env,callbacks){this.env=env;this.cb=callbacks;this.connected=false;this.pending=false;this.latest=null;this.reference=null;this.valid=false;this.status='off';this.timer=null;this.readingCount=0;this.eventCount=0;this.errorName='';this.listener=e=>this.receive(e);this.visibility=()=>{if(env.document.hidden&&this.connected){this.reference=null;this.latest=null;this.invalidate('paused');}};this.screenChange=()=>{if(this.connected){this.reference=null;this.latest=null;this.invalidate('rotated');}};}
    now(){return this.env.performance.now();}
    setStatus(value){if(this.status!==value){this.status=value;this.cb.status?.(value,this);}}
    invalidate(reason){this.valid=false;this.cb.validity?.(false);this.setStatus(reason);}
    async connect(){
      if(this.connected||this.pending)return;
      const e=this.env;this.pending=true;this.setStatus('permission');
      try{
        if(support?.isDesktop(e.navigator))throw new Error('desktop');
        if(!e.isSecureContext)throw new Error('secure');
        if(typeof e.DeviceOrientationEvent==='undefined')throw new Error('unsupported');
        const policy=e.document.permissionsPolicy||e.document.featurePolicy;
        if(policy&&['accelerometer','gyroscope'].some(name=>policy.allowsFeature(name)===false))throw new Error('blocked');
        if(typeof e.DeviceOrientationEvent.requestPermission==='function'){
          const permission=await e.DeviceOrientationEvent.requestPermission();
          if(permission!=='granted')throw new Error('denied');
        }
        this.connected=true;this.latest=null;this.reference=null;this.lastSample=0;this.started=this.now();this.valid=false;this.filtered=null;this.readingCount=0;this.eventCount=0;this.errorName='';
        e.addEventListener('deviceorientation',this.listener);e.document.addEventListener('visibilitychange',this.visibility);e.screen?.orientation?.addEventListener?.('change',this.screenChange);
        this.timer=e.setInterval(()=>this.check(),250);this.cb.validity?.(false);this.setStatus('waiting');
      }catch(error){this.connected=false;this.errorName=error.name||'Error';this.setStatus(['desktop','secure','unsupported','blocked','denied'].includes(error.message)?error.message:'error');}
      finally{this.pending=false;this.cb.status?.(this.status,this);}
    }
    receive(event){
      if(!this.connected||this.env.document.hidden)return;
      this.eventCount++;
      if(!Number.isFinite(event.beta)||!Number.isFinite(event.gamma))return;
      this.readingCount++;
      const now=this.now(),angle=this.env.screen?.orientation?.angle??this.env.orientation??0;
      this.latest={matrix:rotation(Number.isFinite(event.alpha)?event.alpha:0,event.beta,event.gamma,Number(angle)||0),time:now,complete:Number.isFinite(event.alpha)};this.lastSample=now;this.cb.orientation?.(this.latest);
      if(!this.reference){this.invalidate('ready');return;}
      const raw=tilt(this.reference,this.latest.matrix);
      if(Math.abs(raw.pitchDeg)>60||Math.abs(raw.rollDeg)>85){this.invalidate('range');return;}
      const dt=Math.max(0.001,Math.min(0.2,(now-(this.filterTime??now))/1000));
      const weight=1-Math.exp(-dt/0.06);
      if(!this.filtered)this.filtered={...raw};else{this.filtered.pitchDeg+=weight*(raw.pitchDeg-this.filtered.pitchDeg);this.filtered.rollDeg+=weight*angleDelta(raw.rollDeg,this.filtered.rollDeg);this.filtered.yawDeg=angleDelta(this.filtered.yawDeg+weight*angleDelta(raw.yawDeg,this.filtered.yawDeg),0);}
      this.filterTime=now;this.valid=true;this.cb.sample?.({...this.filtered,timestampMs:now});this.cb.validity?.(true);this.setStatus('live');
    }
    calibrate(){
      if(!this.connected||!this.latest||this.now()-this.latest.time>2500||this.env.document.hidden)return false;
      this.reference=this.latest.matrix.slice();this.filtered={pitchDeg:0,rollDeg:0,yawDeg:0};this.filterTime=this.now();this.valid=true;
      this.cb.sample?.({pitchDeg:0,rollDeg:0,yawDeg:0,timestampMs:this.now()});this.cb.validity?.(true);this.setStatus('live');return true;
    }
    check(){if(!this.connected)return;const age=this.now()-(this.lastSample||this.started);if(!this.latest&&age>6000)this.invalidate('unavailable');else if(this.latest&&age>2500)this.invalidate('stale');}
    diagnostics(){const e=this.env,policy=e.document.permissionsPolicy||e.document.featurePolicy;const allowed=name=>{try{return policy?.allowsFeature(name)===false?'Blocked':policy?'Allowed':'Not reported';}catch{return 'Not reported';}};return {secure:e.isSecureContext?'Yes':'No',embedded:e.top&&e.self?e.top!==e.self:false,orientationAPI:typeof e.DeviceOrientationEvent!=='undefined'?'Available':'Unavailable',accelerometer:allowed('accelerometer'),gyroscope:allowed('gyroscope'),events:this.eventCount,readings:this.readingCount,ageMs:this.latest?Math.round(this.now()-this.latest.time):null,calibrated:!!this.reference,mount:'Upright panel: forward into screen',yawDeg:this.valid?this.filtered?.yawDeg:null,status:this.status,error:this.errorName};}
    disconnect(){const e=this.env;e.removeEventListener('deviceorientation',this.listener);e.document.removeEventListener('visibilitychange',this.visibility);e.screen?.orientation?.removeEventListener?.('change',this.screenChange);if(this.timer!==null)e.clearInterval(this.timer);this.timer=null;this.connected=false;this.reference=null;this.valid=false;this.pending=false;this.setStatus('off');}
  }
  const api={rotation,tilt,multiply,transpose,angleDelta,availability,PhoneAttitudeSource};
  if(typeof module!=='undefined'&&module.exports)module.exports=api;else global.PhoneAttitude=api;
})(typeof window!=='undefined'?window:globalThis);
