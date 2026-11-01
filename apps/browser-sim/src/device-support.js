/* Browser input policy, separate from instruments and sensor mathematics.
   Device type only selects the UI path; actual sensor readings establish validity.
   No permission prompts, sensor subscriptions, UA transmission or persistence. */
(function(global){
  function deviceKind(nav={}){
    const ua=String(nav.userAgent||''),platform=String(nav.userAgentData?.platform||nav.platform||'');
    const ipad=/iPad/i.test(ua)||(/Macintosh|MacIntel|macOS/i.test(ua+' '+platform)&&Number(nav.maxTouchPoints)>1);
    if(ipad||nav.userAgentData?.mobile===true||/Android|iPhone|iPod|Windows Phone|Mobile|Silk/i.test(ua)||/Android|iOS/i.test(platform))return 'handheld';
    if(/Windows|Win32|Win64|Macintosh|MacIntel|macOS|Linux|X11|CrOS|Chrome OS/i.test(ua+' '+platform))return 'desktop';
    if(/Tablet/i.test(ua))return 'handheld';
    return 'unknown';
  }
  const isDesktop=nav=>deviceKind(nav)==='desktop';
  function motionAvailability(env){
    const device=deviceKind(env.navigator),deny=(reason,message)=>({allowed:false,device,reason,message});
    if(device==='desktop')return deny('desktop','Desktop demo mode. Phone motion input is disabled here. Use the demo, sliders or heading controls. For live motion, open this page on a phone or supported tablet.');
    if(!env.isSecureContext)return deny('secure','Motion input needs the secure phone version. Open it directly in your phone’s browser.');
    if(typeof env.DeviceOrientationEvent==='undefined')return deny('unsupported','This browser does not expose device orientation. Use the demo controls, or open this page in a supported phone browser.');
    const policy=env.document?.permissionsPolicy||env.document?.featurePolicy;
    try{if(policy&&['accelerometer','gyroscope'].some(name=>policy.allowsFeature(name)===false))return deny('blocked','This browser view blocks motion sensors. Open the secure phone version directly in your phone’s browser.');}catch{/* Unknown policy: the sensor adapter will report the actual result. */}
    return {allowed:true,device,reason:'available',message:'Phone motion input can be requested. Usable sensor readings are required before instruments become live.'};
  }
  const api={deviceKind,isDesktop,motionAvailability};
  if(typeof module!=='undefined'&&module.exports)module.exports=api;else global.DeviceSupport=api;
})(typeof window!=='undefined'?window:globalThis);
