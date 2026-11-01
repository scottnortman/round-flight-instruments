/* Bench turn-coordinator model. References: FAA PHAK ch.8, Fig.8-21;
   Mid-Continent 1394T100 manual 9020079 rev.A, section 4.
   Conventional cues; prototype damping/30-degree cant, not a calibrated 1394T100 model.
   Body frame: forward into upright screen, right screen-right, down screen-bottom.
   All processing is local. No recording or network transmission. */
(function(global){
  const support=typeof module!=='undefined'&&module.exports?require('./device-support.js'):global.DeviceSupport;
  const math=typeof module!=='undefined'&&module.exports?require('./phone-attitude.js'):global.PhoneAttitude;
  const RAD=Math.PI/180,DEG=180/Math.PI;
  const clamp=(v,a,b)=>Math.max(a,Math.min(b,v));
  function ratesFromMotion(rate,screenAngle=0){
    if(!rate||![rate.alpha,rate.beta,rate.gamma].every(Number.isFinite))return null;
    const a=screenAngle*RAD,c=Math.cos(a),s=Math.sin(a);
    return {p:-rate.alpha,q:c*rate.beta-s*rate.gamma,r:-s*rate.beta-c*rate.gamma};
  }
  function ratesFromMatrices(previous,current,dt){
    if(!(dt>=0.005&&dt<=0.25))return null;
    const m=math.multiply(math.transpose(previous),current),theta=Math.acos(clamp((m[0]+m[4]+m[8]-1)/2,-1,1));
    if(theta>Math.PI/2)return null;
    const scale=(theta<1e-6?0.5:theta/(2*Math.sin(theta)))*DEG/dt;
    const x=(m[7]-m[5])*scale,y=(m[2]-m[6])*scale,z=(m[3]-m[1])*scale;
    return {p:-z,q:x,r:-y};
  }
  function ballFromAcceleration(accel,screenAngle=0){
    if(!accel||![accel.x,accel.y,accel.z].every(Number.isFinite))return null;
    const norm=Math.hypot(accel.x,accel.y,accel.z),a=screenAngle*RAD;
    const right=Math.cos(a)*accel.x-Math.sin(a)*accel.y,up=Math.sin(a)*accel.x+Math.cos(a)*accel.y;
    if(norm<3||norm>30||up<=1)return null;
    // Ball follows apparent gravity, opposite measured specific force.
    return clamp(Math.atan2(-right,up)*DEG/15,-1,1);
  }
  const effectiveRate=(p,r)=>r+p*Math.tan(30*RAD);
  const deflection=rate=>clamp(rate/3*20,-45,45);
  const ballPosition=value=>{const angle=clamp(value,-1,1)*17*RAD;return {x:240+350*Math.sin(angle),y:20+350*Math.cos(angle)};};
  class PhoneTurnSource {
    constructor(env){this.env=env;this.connected=false;this.pending=false;this.attempt=0;this.status='off';this.motionCount=0;this.listener=e=>this.receiveMotion(e);this.change=()=>this.reset();this.reset();}
    now(){return this.env.performance.now();}
    screenAngle(){return Number(this.env.screen?.orientation?.angle??this.env.orientation??0)||0;}
    reset(){this.resetTime=this.now();this.previous=null;this.rate=null;this.rateTime=null;this.gyroTime=null;this.ball=null;this.ballTime=null;this.rateSource='none';}
    async connect(){
      if(this.connected||this.pending)return;const attempt=++this.attempt;this.pending=true;this.status='permission';
      try{
        if(support?.isDesktop(this.env.navigator))throw new Error('desktop');
        if(!this.env.isSecureContext)throw new Error('secure');
        const policy=this.env.document.permissionsPolicy||this.env.document.featurePolicy;
        if(policy&&['accelerometer','gyroscope'].some(k=>policy.allowsFeature(k)===false))throw new Error('blocked');
        const EventType=this.env.DeviceMotionEvent;
        if(typeof EventType?.requestPermission==='function'&&await EventType.requestPermission()!=='granted')throw new Error('denied');
        if(attempt!==this.attempt)return;
        this.reset();this.motionCount=0;this.connected=true;this.status=EventType?'waiting':'orientation-only';
        this.env.addEventListener('devicemotion',this.listener);
        this.env.document.addEventListener('visibilitychange',this.change);
        this.env.screen?.orientation?.addEventListener?.('change',this.change);
      }catch(error){if(attempt===this.attempt){this.connected=false;this.status=['desktop','secure','blocked','denied'].includes(error.message)?error.message:'error';}}finally{if(attempt===this.attempt)this.pending=false;}
    }
    acceptRate(rate,time,source){
      if(!rate||Object.values(rate).some(v=>!Number.isFinite(v)||Math.abs(v)>720))return;
      const dt=this.rateTime===null?0.2:clamp((time-this.rateTime)/1000,0,0.2),weight=1-Math.exp(-dt/0.22);
      if(!this.rate||time-this.rateTime>1000)this.rate={...rate};else for(const k of ['p','q','r'])this.rate[k]+=weight*(rate[k]-this.rate[k]);
      this.rateTime=time;this.rateSource=source;this.status='live';
    }
    receiveMotion(event){
      if(!this.connected||this.env.document.hidden)return;this.motionCount++;
      const now=this.now(),rate=ratesFromMotion(event.rotationRate,this.screenAngle());
      if(rate&&Object.values(rate).every(v=>Math.abs(v)<=720)){this.acceptRate(rate,now,'phone gyro');this.gyroTime=now;}
      const ball=ballFromAcceleration(event.accelerationIncludingGravity,this.screenAngle());
      if(ball!==null){const weight=this.ball===null||now-this.ballTime>1000?1:1-Math.exp(-clamp((now-this.ballTime)/1000,0,0.2)/0.25);this.ball=(this.ball??ball)+weight*(ball-(this.ball??ball));this.ballTime=now;}
      else{this.ball=null;this.ballTime=null;}
    }
    receiveOrientation(reading){
      if(!this.connected||this.env.document.hidden||reading.complete===false){this.previous=null;return;}
      const previous=this.previous;this.previous=reading;
      if(!previous||this.gyroTime!==null&&reading.time-this.gyroTime<500)return;
      this.acceptRate(ratesFromMatrices(previous.matrix,reading.matrix,(reading.time-previous.time)/1000),reading.time,'orientation-derived');
    }
    snapshot(){const now=this.now(),visible=this.connected&&!this.env.document.hidden;return {p:this.rate?.p??0,q:this.rate?.q??0,r:this.rate?.r??0,ball:this.ball??0,valid:visible&&this.rateTime!==null&&now-this.rateTime<1500,ballValid:visible&&this.ballTime!==null&&now-this.ballTime<1500,pending:visible&&this.rateTime===null&&now-this.resetTime<2000,source:this.rateSource,ballSource:'phone acceleration',motionCount:this.motionCount,status:this.status};}
    disconnect(){this.attempt++;this.pending=false;this.env.removeEventListener('devicemotion',this.listener);this.env.document.removeEventListener('visibilitychange',this.change);this.env.screen?.orientation?.removeEventListener?.('change',this.change);this.connected=false;this.status='off';this.reset();}
  }
  const api={ratesFromMotion,ratesFromMatrices,ballFromAcceleration,effectiveRate,deflection,ballPosition,PhoneTurnSource};
  if(typeof module!=='undefined'&&module.exports)module.exports=api;else global.TurnCoordinator=api;
})(typeof window!=='undefined'?window:globalThis);
