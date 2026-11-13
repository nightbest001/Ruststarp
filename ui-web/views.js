/* views: settings, home, launch (bootstrapper), setup (installer), about */
(function(){
const D=window.__INIT;const NAME=D.name;const WIKI=D.links.wiki;
const root=document.getElementById('root');
const F={MSAA:'FIntDebugForceMSAASamples',TEXON:'DFFlagTextureQualityOverrideEnabled',TEXLV:'DFIntTextureQualityOverride',SCALE:'DFFlagDisableDPIScale',FS:'FFlagHandleAltEnterFullscreenManually'};
const PRESET_FLAGS=Object.values(F);
const wrap=(win,...kids)=>{win.append(...kids);return win};

/* =========================================================== UPDATE BANNER */
const mb=n=>n?(n/1048576).toFixed(1)+' MB':'';
function UpdateBar(o){
  o=o||{};
  const el=h('div',{class:'upd'});el.style.display='none';
  const S={busy:false};
  const title=h('div',{class:'tt'});const sub=h('div',{class:'ds'});
  const pbv=h('div',{class:'v'});const pb=h('div',{class:'pb',style:{display:'none',marginTop:'8px'}},pbv);
  const go=Btn('',{kind:'primary',icon:'ArrowDownload16',onclick:()=>run()});
  const later=Btn('Later',{onclick:()=>hide()});
  const label=txt=>{go.querySelector('span').textContent=txt};
  const show=()=>{el.style.display='';if(D.mode==='home')call('win',{op:'banner',on:true})};
  function hide(){el.style.display='none';if(D.mode==='home')call('win',{op:'banner',on:false})}
  function fail(e){S.busy=false;go.disabled=false;go.classList.remove('dis');later.style.display='';
    pb.classList.remove('ind');pb.classList.add('err');pbv.style.width='100%';
    title.textContent='Update failed';sub.textContent=String(e||'Unknown error');label('Try again')}
  async function run(){
    if(S.busy)return;
    if(o.beforeRun&&!(await o.beforeRun()))return;
    S.busy=true;go.disabled=true;go.classList.add('dis');later.style.display='none';
    pb.style.display='';pb.classList.remove('err');pb.classList.add('ind');
    title.textContent='Updating '+NAME;sub.textContent='Starting...';
    try{await call('start_update')}catch(e){return fail(e)}
    poll()}
  async function poll(){
    let p;try{p=await call('update_progress')}catch(e){return fail(e)}
    if(!p)return setTimeout(poll,150);
    if(p.phase==='failed')return fail(p.error);
    if(p.phase==='downloading'){
      if(p.total){pb.classList.remove('ind');pbv.style.width=Math.min(100,p.done*100/p.total)+'%'}
      sub.textContent='Downloading '+mb(p.done)+(p.total?' of '+mb(p.total):'')}
    else{pb.classList.add('ind');sub.textContent=p.phase==='done'?'Restarting...':'Installing...'}
    setTimeout(poll,150)}
  /* manual=true: tell the user when there is nothing new or the check failed */
  el.check=async function(manual){
    try{
      const r=await call('check_update');
      if(r&&r.available){
        title.textContent='Update available: version '+r.version;
        sub.textContent='You have '+r.current+(r.size?' ('+mb(r.size)+')':'')+'. The update installs right here, no browser download needed.';
        label('Update to '+r.version);show();return true}
      if(manual)snack({title:'You are up to date',msg:NAME+' '+D.version+' is the newest version.',kind:'ok',icon:'CheckmarkCircle32'});
    }catch(e){if(manual)snack({title:'Could not check for updates',msg:String(e),kind:'bad',timeout:6000})}
    return false};
  el.append(icon('ArrowDownload16','ui'),h('div',{class:'ut'},title,sub,pb),go,later);
  return el}

/* =========================================================== SETTINGS */
function Settings(){
  const S={s:D.settings,force:D.force_reinstall,flags:D.flags,sc:D.shortcuts.slice(),sel:-1,showPreset:false,filter:'',selFlag:null};
  const snap=()=>JSON.stringify([S.s,S.force,S.flags,S.sc]);let saved=snap();
  const dirty=()=>snap()!==saved;
  let page='integrations';let crumbs=[];
  const win=h('div',{class:'win'});
  window.__themeMode=S.s.theme;applyTheme(S.s.theme);
  const upd=UpdateBar({beforeRun:async()=>!dirty()||await confirmBox(t('Common.Warning'),t('Menu.UnsavedChanges'))});

  const NAV=[
    ['integrations','Menu.Integrations.Title','Add28'],['behaviour','Menu.Behaviour.Title','PlaySettings20'],['mods','Menu.Mods.Title','WrenchScrewdriver20'],
    ['fastflags','Menu.FastFlags.Title','Flag24'],['appearance','Menu.Appearance.Title','PaintBrush24'],['shortcuts','Common.Shortcuts','Apps28'],['gpu','GPU','DeveloperBoard24']];
  const navEl=h('div',{class:'nav'});const crumb=h('div',{class:'crumb'});const pageWrap=h('div',{class:'page-wrap'});
  const items={};
  function navItem(id,label,ic,o){const el=h('div',{class:'nav-item'+(o&&o.last?' last':''),onclick:()=>(o&&o.action)?o.action():go(id)},icon(ic),h('span',null,label));items[id]=el;return el}
  NAV.forEach(n=>navEl.append(navItem(n[0],n[1]==='GPU'?'GPU':t(n[1]),n[2])));
  navEl.append(h('div',{class:'grow'}));
  navEl.append(navItem('ruststrap',NAME,'Settings24'));
  navEl.append(navItem('about',t('Menu.About.Title'),'QuestionCircle48',{last:true}));

  function go(p,fromBack){
    clearRegs();page=p;
    const active=p==='fastflageditor'?'fastflags':p;Object.entries(items).forEach(([k,e])=>e.classList.toggle('active',k===active));
    crumb.innerHTML='';
    const title=({integrations:t('Menu.Integrations.Title'),behaviour:t('Menu.Behaviour.Title'),mods:t('Menu.Mods.Title'),fastflags:t('Menu.FastFlags.Title'),fastflageditor:t('Menu.FastFlagEditor.Title'),
      appearance:t('Menu.Appearance.Title'),shortcuts:t('Common.Shortcuts'),gpu:'GPU',ruststrap:NAME,about:t('Menu.About.Title')})[p];
    if(p==='fastflageditor'){crumb.append(h('span',{class:'part link',onclick:()=>go('fastflags')},t('Menu.FastFlags.Title')),h('span',{class:'sep'},icon('ChevronRight20')),h('span',null,title))}
    else crumb.append(h('span',null,title));
    const body=PAGES[p]();pageWrap.innerHTML='';pageWrap.scrollTop=0;pageWrap.append(body);touch()}

  /* --- status bar --- */
  const saveBtn=Btn(t('Menu.Save'),{kind:'primary',onclick:save});
  const status=h('div',{class:'status'},h('div',{class:'l'}),saveBtn,Btn(t('Common.Close'),{onclick:()=>closeWin()}));
  async function save(){
    try{const r=await call('save',{settings:S.s,force_reinstall:S.force,flags:S.flags,shortcuts:S.sc});
      saved=snap();touch();
      if(r&&r.failed&&r.failed.length)snack({title:t('Menu.Save'),msg:r.failed.join('\n'),kind:'bad',icon:'Info20',timeout:6000});
      else snack({title:t('Menu.SettingsSaved.Title'),msg:t('Menu.SettingsSaved.Message'),kind:'ok',icon:'CheckmarkCircle32'})}
    catch(e){snack({title:'Error',msg:String(e),kind:'bad',timeout:6000})}}
  async function closeWin(){
    if(dirty()){const ok=await confirmBox(t('Common.Warning'),t('Menu.UnsavedChanges'));if(!ok)return}
    call('win',{op:'close',force:true})}
  window.__tryClose=closeWin;
  onTouch(()=>{if(S.s.theme!==window.__themeMode){window.__themeMode=S.s.theme;applyTheme(S.s.theme)}});

  /* --- helpers --- */
  const sec=(k)=>h('div',{class:'h2'},t(k));
  const pg=(...k)=>h('div',{class:'page narrow'},k);
  const dsc=(k,extra)=>h('div',{class:'desc'},md(k),extra);
  const flag=k=>S.flags[k]!=null?String(S.flags[k]):null;
  const setFlag=(k,v)=>{if(v==null)delete S.flags[k];else S.flags[k]=String(v)};

  const PAGES={
    integrations(){
      const list=h('div',{class:'lb'});
      const right=h('div',{class:'rcol'});
      const none=h('div',{class:'none'},t('Menu.Integrations.Custom.NoneSelected'));
      const paint=()=>{list.innerHTML='';S.s.custom_integrations.forEach((c,i)=>list.append(h('div',{class:'it'+(i===S.sel?' sel':''),onclick:()=>{S.sel=i;paint()}},c.name||t('Menu.Integrations.Custom.NewIntegration'))));
        right.innerHTML='';const c=S.s.custom_integrations[S.sel];
        if(!c){right.append(none);return}
        const loc=Text(()=>c.location,v=>c.location=v,{placeholder:'C:\\Windows\\System32\\cmd.exe'});
        right.append(h('div',{class:'lbl'},t('Common.Name')),Text(()=>c.name,v=>{c.name=v;const it=list.children[S.sel];if(it)it.textContent=v||t('Menu.Integrations.Custom.NewIntegration')}),
          h('div',{class:'lbl',style:{marginTop:'8px'}},t('Menu.Integrations.Custom.AppLocation')),
          h('div',{style:{display:'grid',gridTemplateColumns:'1fr auto',gap:'8px'}},loc,Btn(t('Common.Browse'),{icon:'Folder24',onclick:async()=>{const p=await call('browse_file');if(p){c.location=p;loc.input.value=p;touch()}}})),
          h('div',{class:'lbl',style:{marginTop:'8px'}},t('Menu.Integrations.Custom.LaunchArgs')),
          Text(()=>c.launch_args,v=>c.launch_args=v,{multi:true,rows:3,placeholder:'/k echo '+t('Menu.Integrations.Custom.LaunchArgs.Placeholder')}),
          h('div',{style:{marginTop:'8px'}},Check(t('Menu.Integrations.Custom.AutoClose'),()=>c.auto_close,v=>c.auto_close=v)))};
      const act=()=>S.s.enable_activity_tracking;
      const el=pg(dsc('Menu.Integrations.Description'),sec('Menu.Integrations.ActivityTracking'),
        Option({header:t('Menu.Integrations.EnableActivityTracking.Title'),desc:t('Menu.Integrations.EnableActivityTracking.Description'),help:WIKI+'/features/activity-tracking/',content:Toggle(()=>S.s.enable_activity_tracking,v=>S.s.enable_activity_tracking=v)}),
        Option({header:t('Menu.Integrations.DesktopApp.Title'),desc:t('Menu.Integrations.DesktopApp.Description',WIKI+'/features/dont-exit-to-desktop-app/'),help:WIKI+'/features/dont-exit-to-desktop-app/',enabled:act,content:Toggle(()=>S.s.use_disable_app_patch,v=>S.s.use_disable_app_patch=v)}),
        sec('Menu.Integrations.Custom.Title'),h('div',{class:'desc',style:{margin:0}},t('Menu.Integrations.Custom.Description')),
        h('div',{class:'cint'},h('div',{class:'lcol'},list,h('div',{class:'btnrow',style:{margin:0}},
          Btn(t('Common.New'),{icon:'Add28',onclick:()=>{S.s.custom_integrations.push({name:t('Menu.Integrations.Custom.NewIntegration'),location:'',launch_args:'',auto_close:true});S.sel=S.s.custom_integrations.length-1;paint();touch()}}),
          Btn(t('Common.Delete'),{icon:'Delete28',kind:'danger',enabled:()=>S.sel>=0,onclick:()=>{S.s.custom_integrations.splice(S.sel,1);S.sel=-1;paint();touch()}}))),right));
      paint();return el},

    behaviour(){return pg(dsc('Menu.Behaviour.Description'),
      Option({header:t('Menu.Behaviour.ConfirmLaunches.Title'),desc:t('Menu.Behaviour.ConfirmLaunches.Description'),content:Toggle(()=>S.s.confirm_launches,v=>S.s.confirm_launches=v)}),
      Option({header:t('Menu.Behaviour.ForceRobloxReinstall.Title'),desc:t('Menu.Behaviour.ForceRobloxReinstall.Description'),enabled:()=>D.installed,content:Toggle(()=>S.force,v=>S.force=v)}),
      Option({header:'Keep downloaded packages',desc:'Keeps old package files in the cache instead of deleting them after an update.',content:Toggle(()=>S.s.debug_disable_version_package_cleanup,v=>S.s.debug_disable_version_package_cleanup=v)}))},

    mods(){return pg(dsc('Menu.Mods.Description'),
      h('div',{class:'grid2'},
        CardAction({icon:'Folder24',title:t('Menu.Mods.OpenModsFolder.Title'),desc:t('Menu.Mods.OpenModsFolder.Description'),onclick:()=>call('open_folder',{which:'mods'})}),
        CardAction({icon:'BookQuestionMark24',title:t('Common.Help'),desc:t('Menu.Mods.Help.Description'),onclick:()=>openUrl(WIKI+'/features/mods/')})),
      Card(h('div',{style:{fontSize:'14px'}},'How it works'),h('div',{class:'t3',style:{fontSize:'12px',marginTop:'2px',lineHeight:1.4}},
        'Every file you put in the mods folder replaces the file with the same path inside the Roblox folder. For example, content\\sounds\\ouch.ogg changes the death sound. Delete the file and the original comes back the next time Roblox starts through '+NAME+'.')))},

    fastflags(){
      const msaa=Combo([{v:'',label:t('Common.Automatic')},{v:'1',label:'1x'},{v:'2',label:'2x'},{v:'4',label:'4x'}],()=>flag(F.MSAA)||'',v=>setFlag(F.MSAA,v||null));
      const texOn=()=>flag(F.TEXON)!=null;
      const tex=Combo([{v:'',label:t('Common.Automatic')},{v:'0',label:'Level 0'},{v:'1',label:'Level 1'},{v:'2',label:'Level 2'},{v:'3',label:'Level 3'}],()=>texOn()?(flag(F.TEXLV)||''):'',v=>{if(!v){setFlag(F.TEXON,null);setFlag(F.TEXLV,null)}else{setFlag(F.TEXON,'True');setFlag(F.TEXLV,v)}});
      return pg(h('div',{class:'desc',style:{marginBottom:'16px'}},t('Menu.FastFlags.Description')),
        Option({header:t('Menu.FastFlags.ManagerEnabled.Title'),desc:t('Menu.FastFlags.ManagerEnabled.Description'),content:Toggle(()=>S.s.use_fast_flag_manager,v=>S.s.use_fast_flag_manager=v)}),
        sec('Common.Presets'),h('div',{class:'h3'},t('Menu.FastFlags.Presets.Categories.Rendering')),
        h('div',{class:'desc',style:{marginTop:'4px'}},md('Menu.FastFlags.Presets.D3DExclusiveFullscreenInfo',WIKI+'/features/exclusive-fullscreen/')),
        Option({header:t('Menu.FastFlags.Presets.MSAA.Title'),content:msaa}),
        Option({header:t('Menu.FastFlags.Presets.FixDisplayScaling.Title'),desc:t('Menu.FastFlags.Presets.FixDisplayScaling.Description'),content:Toggle(()=>flag(F.SCALE)==='True',v=>setFlag(F.SCALE,v?'True':null))}),
        Option({header:t('Menu.FastFlags.Presets.TextureQuality.Title'),content:tex}),
        h('div',{style:{height:'24px'}}),
        Option({header:t('Menu.FastFlags.Reset.Title'),content:Btn(t('Common.Reset'),{onclick:async()=>{if(await confirmBox(t('Common.Warning'),t('Menu.FastFlags.Reset.Title')+'?')){S.flags={};touch();go('fastflags')}}})}),
        CardAction({icon:'EditSettings24',title:t('Menu.FastFlagEditor.Title'),desc:t('Menu.FastFlags.Editor.Description'),onclick:()=>go('fastflageditor')}))},

    fastflageditor(){
      const body=h('div',{class:'body'});const dg=h('div',{class:'dg'});
      const rows=()=>Object.keys(S.flags).filter(k=>(S.showPreset||!PRESET_FLAGS.includes(k))&&(!S.filter||k.toLowerCase().includes(S.filter.toLowerCase()))).sort();
      function edit(cell,get,set){const inp=h('input',{value:get()});cell.innerHTML='';cell.append(inp);inp.focus();inp.select();
        const done=ok=>{if(ok)set(inp.value);paint()};inp.addEventListener('blur',()=>done(true));inp.addEventListener('keydown',e=>{if(e.key==='Enter')inp.blur();if(e.key==='Escape'){inp.removeEventListener('blur',()=>{});done(false)}})}
      function paint(){body.innerHTML='';const ks=rows();
        if(!ks.length)body.append(h('div',{class:'empty'},t('Common.NotAvailable')));
        ks.forEach(k=>{const n=h('div',{title:k},k);const v=h('div',{title:String(S.flags[k])},String(S.flags[k]));
          const r=h('div',{class:'row b'+(S.selFlag===k?' sel':''),onclick:()=>{S.selFlag=k;body.querySelectorAll('.row.sel').forEach(x=>x.classList.remove('sel'));r.classList.add('sel');touch()}},n,v);
          n.addEventListener('dblclick',()=>edit(n,()=>k,nv=>{nv=nv.trim();if(nv&&nv!==k&&!(nv in S.flags)){const val=S.flags[k];delete S.flags[k];S.flags[nv]=val;S.selFlag=nv}touch()}));
          v.addEventListener('dblclick',()=>edit(v,()=>String(S.flags[k]),nv=>{S.flags[k]=nv;touch()}));body.append(r)})}
      const showBtn=Btn(t('Menu.FastFlagEditor.ShowPresetFlags'),{onclick:()=>{S.showPreset=!S.showPreset;showBtn.classList.toggle('primary',S.showPreset);paint()}});showBtn.classList.toggle('primary',S.showPreset);
      dg.append(h('div',{class:'row h'},h('div',null,t('Common.Name')),h('div',null,t('Common.Value'))),body);
      const bar=h('div',{style:{display:'flex',flexWrap:'wrap',gap:'12px',marginBottom:'16px'}},
        Btn(t('Menu.FastFlagEditor.Back'),{icon:'ArrowLeft48',onclick:()=>go('fastflags')}),
        Btn(t('Menu.FastFlagEditor.AddNew'),{icon:'Add28',onclick:addFlag}),
        Btn(t('Menu.FastFlagEditor.DeleteSelected'),{icon:'Delete48',enabled:()=>S.selFlag!=null&&S.selFlag in S.flags,onclick:()=>{delete S.flags[S.selFlag];S.selFlag=null;paint();touch()}}),
        showBtn,
        Btn(t('Menu.FastFlagEditor.ExportJson'),{icon:'ArrowExportRtl24',onclick:async()=>{const o={};rows().forEach(k=>o[k]=S.flags[k]);try{await navigator.clipboard.writeText(JSON.stringify(o,null,4));snack({title:t('Menu.FastFlagEditor.JsonCopiedToClipboard'),icon:'CheckmarkCircle32',kind:'ok'})}catch(e){snack({title:String(e),kind:'bad'})}}}));
      const search=Text(()=>S.filter,v=>{S.filter=v;paint()},{icon:'Search32',placeholder:t('Menu.FastFlagEditor.Search')});search.style.marginBottom='16px';
      paint();
      return pg(h('div',{class:'desc',style:{marginBottom:'2px'}},t('Menu.FastFlagEditor.Description')),h('div',{class:'desc',style:{marginBottom:'12px'}},md('Menu.FastFlagEditor.AllowlistInformation',WIKI+'/features/fastflag-allowlist/')),
        h('div',{class:'ib'},icon('Warning28'),h('div',null,h('div',{class:'tt'},t('Menu.FastFlagEditor.BannerTitle')),h('div',{class:'ds'},t('Menu.FastFlagEditor.BannerText')))),bar,search,dg)},

    appearance(){
      const dim=h('div');
      return pg(dsc('Menu.Appearance.Description'),
        Option({header:t('Menu.Appearance.Global.Theme.Title'),content:Combo([{v:'system',label:t('Common.SystemDefault')},{v:'light',label:'Light'},{v:'dark',label:'Dark'}],()=>S.s.theme,v=>S.s.theme=v)}),
        sec('Menu.Appearance.Bootstrapper.Title'),h('div',{class:'desc',style:{margin:0}},t('Menu.Appearance.Bootstrapper.Description')),
        h('div',{style:{marginTop:'16px'}},Btn(t('Menu.Appearance.Preview'),{onclick:()=>previewBoot(S.s.bootstrapper_title)})),
        Expander({title:t('Menu.Appearance.Customisation.Title'),desc:t('Menu.Appearance.Customisation.Description'),body:h('div',null,
          h('div',null,t('Menu.Appearance.CustomisationTitle.Title')),h('div',{style:{marginTop:'4px'}},Text(()=>S.s.bootstrapper_title,v=>S.s.bootstrapper_title=v,{placeholder:NAME})),
          h('div',{class:'t3',style:{fontSize:'12px',marginTop:'4px'}},t('Menu.Appearance.CustomisationTitle.Description')))}))},

    shortcuts(){const opt=(k,i)=>Option({header:k,content:Toggle(()=>S.sc[i],v=>S.sc[i]=v)});
      const g2=(a,b)=>h('div',{class:'grid2'},a,b);
      return pg(h('div',{class:'desc',style:{marginBottom:'16px'}},t('Menu.Shortcuts.Description')),
        sec('Menu.Shortcuts.General.Title'),h('div',{class:'desc',style:{margin:0}},t('Menu.Shortcuts.General.Description')),
        g2(opt(t('Common.Shortcuts.Desktop'),0),opt(t('Common.Shortcuts.StartMenu'),1)),
        sec('Menu.Shortcuts.Function.Title'),h('div',{class:'desc',style:{margin:0}},t('Menu.Shortcuts.Function.Description')),
        h('div',{class:'grid3'},opt(t('LaunchMenu.LaunchRoblox'),2),opt(t('LaunchMenu.LaunchRobloxStudio'),3),opt(t('Menu.Title'),4))) },

    gpu(){
      const kind=a=>({discrete:'discrete',integrated:'integrated'}[a.kind]||'unknown type');
      const cur=Combo([{v:'default',label:'Let Windows decide'},{v:'saving',label:'Power saving GPU'},{v:'high',label:'High performance GPU'}],()=>S.s.gpu_preference,v=>S.s.gpu_preference=v,220);
      const hint=h('div',{class:'t3',style:{fontSize:'12px',marginTop:'8px'}});
      reg(()=>hint.textContent=({default:'No change. Windows picks the GPU.',saving:'Usually the integrated GPU. Longer battery life.',high:'Usually the discrete GPU (NVIDIA / AMD). Best for playing.'})[S.s.gpu_preference]||'');
      return pg(h('div',{class:'desc'},'Pick which graphics card Roblox uses. Useful on laptops with two GPUs.'),sec('Detected'),
        D.adapters.length?D.adapters.map(a=>Card(h('div',null,a.name),h('div',{class:'t3',style:{fontSize:'12px',marginTop:'2px'}},`${a.vendor}  ·  ${kind(a)}  ·  ${a.vram_mb?a.vram_mb+' MB':'VRAM unknown'}`))):Card(h('span',{class:'t2'},'No GPU was found in the registry.')),
        sec('Roblox should use'),Option({header:'Graphics card',desc:'Pick the GPU Roblox runs on.',content:cur}),hint,
        h('div',{class:'t3',style:{fontSize:'12px',marginTop:'8px'}},'Windows keeps this per Roblox version. '+NAME+' writes it again after every update.'))},

    ruststrap(){
      const ec={v:true},el={v:true};
      return pg(h('div',{class:'desc'},t('Menu.Bloxstrap.Description')),
        Option({header:D.installed?'Repair installation':'Install '+NAME,desc:'Copies '+NAME+' to your user folder and makes roblox:// links open through it.',content:Btn(D.installed?'Repair':t('Common.Navigation.Install'),{kind:'primary',onclick:async()=>{try{await call('install');D.installed=true;snack({title:NAME+' installed',kind:'ok',icon:'CheckmarkCircle32'});go('ruststrap')}catch(e){snack({title:'Install failed',msg:String(e),kind:'bad',timeout:6000})}}})}),
        Option({header:t('Menu.Behaviour.AutoUpdate.Title'),desc:t('Menu.Behaviour.AutoUpdate.Description'),content:Toggle(()=>S.s.check_for_updates,v=>S.s.check_for_updates=v)}),
        Option({header:'Check for updates now',desc:'You have version '+D.version+'. Looks on GitHub for a newer release.'+(D.can_update?'':' Open the installed copy of '+NAME+' to update it.'),content:Btn('Check now',{icon:'ArrowDownload16',enabled:()=>D.can_update,onclick:()=>upd.check(true)})}),
        Option({header:'Data folder',desc:D.paths.base,content:Btn('Open',{icon:'Folder24',onclick:()=>call('open_folder',{which:'base'})})}),
        Option({header:'Logs',desc:D.paths.logs,content:Btn(t('Common.OpenLogFile').replace(/ file$/,''),{icon:'Folder24',onclick:()=>call('open_folder',{which:'logs'})})}),
        Card(h('div',{style:{display:'flex',alignItems:'center',gap:'16px'}},h('div',{style:{flex:1}},h('div',null,t('Menu.Bloxstrap.ExportData.Title')),h('div',{class:'t3',style:{fontSize:'12px',marginTop:'2px'}},t('Menu.Bloxstrap.ExportData.Description')),
          h('div',{style:{display:'flex',gap:'24px',marginTop:'12px'}},Check(t('Menu.Bloxstrap.ExportData.ExportConfig'),()=>ec.v,v=>ec.v=v),Check(t('Menu.Bloxstrap.ExportData.ExportLogs'),()=>el.v,v=>el.v=v))),
          Btn(t('Common.Export'),{icon:'CopySelect20',cls:'',enabled:()=>ec.v||el.v,onclick:async()=>{try{const r=await call('export_diag',{config:ec.v,logs:el.v});if(r)snack({title:'Diagnostic data exported',kind:'ok',icon:'CheckmarkCircle32'})}catch(e){snack({title:'Export failed',msg:String(e),kind:'bad',timeout:6000})}}}))),
        Option({header:'Uninstall',desc:'Removes '+NAME+', its shortcuts and the Roblox it installed. You can also do this from Windows Settings > Apps.',content:Btn('Uninstall',{kind:'danger',icon:'Delete28',enabled:()=>D.installed,onclick:async()=>{
          const keep={v:false};
          const body=h('div',null,h('div',null,NAME+' and the Roblox it installed will be removed.'),h('div',{style:{marginTop:'12px'}},Check(t('Uninstaller.KeepData.Label'),()=>keep.v,v=>keep.v=v)),h('div',{class:'t3',style:{fontSize:'12px',marginTop:'4px'}},t('Uninstaller.KeepData.Description')));
          if(await confirmBox('Uninstall '+NAME+'?',body,t('Uninstaller.Uninstall'),t('Common.Cancel'))){
            try{await call('uninstall',{keep_data:keep.v});await dialog({title:'Uninstalled',body:NAME+' has been removed.'});call('win',{op:'close',force:true})}
            catch(e){snack({title:'Uninstall failed',msg:String(e),kind:'bad',timeout:6000})}}}})}))},

    about(){return AboutBody()}
  };

  function addFlag(){
    let tab=0;const name={v:''},val={v:''},json={v:''};
    const tabs=h('div',{class:'tabs'});const content=h('div');const ok=Btn(t('Common.OK'),{kind:'primary'});
    const upd=()=>{tabs.innerHTML='';[t('Common.AddSingle'),t('Common.ImportJson')].forEach((l,i)=>tabs.append(h('div',{class:'tab'+(i===tab?' on':''),onclick:()=>{tab=i;upd()}},l)));content.innerHTML='';
      if(tab===0)content.append(h('div',{class:'frow'},h('div',{class:'k'},t('Common.Name')),h('div',{style:{flex:1}},Text(()=>name.v,v=>{name.v=v;chk()}))),h('div',{class:'frow'},h('div',{class:'k'},t('Common.Value')),h('div',{style:{flex:1}},Text(()=>val.v,v=>val.v=v))));
      else{const ta=Text(()=>json.v,v=>{json.v=v;chk()},{multi:true,rows:8});content.append(ta,h('div',{style:{marginTop:'8px'}},Btn(t('Common.ImportFromFile'),{icon:'DocumentArrowUp16',cls:'stretch',onclick:async()=>{const r=await call('read_file');if(r){json.v=r;ta.input.value=r;chk()}}})))}
      chk()};
    const chk=()=>{const good=tab===0?name.v.trim().length>0:json.v.trim().length>0;ok.disabled=!good;ok.classList.toggle('dis',!good)};
    const PRE=/^(FFlag|DFFlag|SFFlag|FInt|DFInt|SFInt|FString|DFString|SFString|FLog|DFLog|SFLog)/;
    const scrim=h('div',{class:'scrim'},h('div',{class:'dlg'},h('div',{class:'dh'},t('Dialog.AddFastFlag.Title')),h('div',{class:'db nw'},tabs,content),
      h('div',{class:'df'},ok,Btn(t('Common.Cancel'),{onclick:()=>scrim.remove()}))));
    ok.addEventListener('click',()=>{
      const bad=(m)=>dialog({title:t('Common.Warning'),body:m});
      const addOne=(n,v)=>{if(!/^[A-Za-z0-9_]+$/.test(n))return t('Menu.FastFlagEditor.InvalidCharacter',n);if(!PRE.test(n))return t('Menu.FastFlagEditor.InvalidPrefix',n);return null};
      if(tab===0){const n=name.v.trim();const e=addOne(n);if(e)return bad(e);if(n in S.flags)return bad(t('Menu.FastFlagEditor.AlreadyExists'));S.flags[n]=val.v.trim()}
      else{let o;try{o=JSON.parse(json.v)}catch(e){return bad(t('Menu.FastFlagEditor.InvalidJSON',e.message))}
        for(const k of Object.keys(o)){const e=addOne(k);if(e)return bad(e)}for(const k of Object.keys(o))S.flags[k]=String(o[k])}
      scrim.remove();go('fastflageditor')});
    document.body.append(scrim);upd()}

  const mainEl=h('div',{class:'main'},crumb,pageWrap);
  const rootEl=h('div',{class:'root'},navEl,mainEl);
  wrap(win,TitleBar({title:t('Menu.Title'),min:true,max:true,onclose:()=>closeWin()}),upd,rootEl,status,ResizeHandles());
  const dirtyLabel=status.querySelector('.l');
  onTouch(()=>{const d=dirty();saveBtn.disabled=false;dirtyLabel.textContent=D.installed?'':'Not installed yet: open the '+NAME+' page and press Install.'});
  root.append(win);go('integrations');
  if(D.can_update&&D.settings.check_for_updates)upd.check(false);
  if(D.already_running)snack({title:t('Menu.AlreadyRunning.Title'),msg:t('Menu.AlreadyRunning.Caption'),icon:'Info20',timeout:10000});
}

/* =========================================================== ABOUT */
function AboutBody(){
  const link=(ic,label,url)=>CardAction({icon:ic,title:label,onclick:()=>openUrl(url)});
  return h('div',{class:'page narrow'},
    h('div',{class:'hero',style:{marginBottom:'8px'}},h('img',{src:window.APP_ICON,alt:''}),h('div',null,h('div',{style:{fontSize:'28px',fontWeight:600}},NAME),h('div',{class:'t3'},t('Menu.About.Version',D.version)))),
    h('div',{class:'desc'},t('Menu.About.Description')),
    h('div',{class:'h2'},t('Menu.About.HelpInformation')),
    h('div',{class:'grid2'},link('Code24',t('Menu.About.GithubRepository'),D.links.repo),link('BookQuestionMark24',t('Menu.About.ReportIssue'),D.links.issues)),
    Expander({icon:'Heart24',title:t('Menu.About.Licenses'),open:false,body:h('div',null,
      h('div',null,NAME+' is a Rust port of Bloxstrap.'),h('div',{class:'t3',style:{marginTop:'6px',fontSize:'12px',lineHeight:1.45}},'The launch, update and mod pipeline comes from Bloxstrap by pizzaboxer and contributors, used under the MIT license.'),
      h('div',{style:{marginTop:'8px'}},h('a',{class:'lnk',onclick:()=>openUrl('https://github.com/bloxstraplabs/bloxstrap')},'Bloxstrap on GitHub')))}))}

/* =========================================================== BOOT DIALOG (Fluent) */
function BootView(o){
  const msg=h('div',{class:'msg'},o.message||'');const pbv=h('div',{class:'v'});const pb=h('div',{class:'pb ind pbw',style:{marginBottom:'16px'}},pbv);
  const cancel=Btn(t('Common.Cancel'),{onclick:o.oncancel});
  const el=h('div',{class:'boot'},h('div',{class:'topbar'}),h('img',{class:'ic',src:window.APP_ICON,alt:''}),h('div',{class:'sp'}),msg,pb,cancel);
  el.set=(s)=>{msg.textContent=s.message;
    if(s.fraction==null){pb.classList.add('ind');pbv.style.width=''}else{pb.classList.remove('ind');pbv.style.width=(s.fraction*100)+'%'}
    cancel.disabled=!s.can_cancel;cancel.classList.toggle('dis',!s.can_cancel);pb.classList.toggle('err',!!s.error)};
  return el}
function previewBoot(title){
  const el=BootView({message:t('Bootstrapper.Status.Connecting'),oncancel:()=>{clearInterval(iv);scrim.remove()}});
  const win=h('div',{class:'win framed',style:{width:'500px',height:'280px',borderRadius:'8px',overflow:'hidden',boxShadow:'0 32px 64px rgba(0,0,0,.37)',animation:'dlgIn .25s cubic-bezier(.1,.9,.2,1)'}},el);
  const scrim=h('div',{class:'scrim'},win);document.body.append(scrim);
  const steps=[[t('Bootstrapper.Status.Connecting'),null],[t('Bootstrapper.Status.Installing',' Roblox').replace('  ',' '),.2],[t('Bootstrapper.Status.Installing','Roblox'),.55],[t('Bootstrapper.Status.ApplyingModifications'),.85],[t('Bootstrapper.Status.Starting','Roblox'),1]];
  let i=0;el.set({message:steps[0][0],fraction:null,can_cancel:true});
  const iv=setInterval(()=>{i=(i+1)%steps.length;el.set({message:steps[i][0],fraction:steps[i][1],can_cancel:true})},1500)}

/* =========================================================== LAUNCH (bootstrapper) */
function Launch(){
  window.__themeMode=D.settings.theme;applyTheme(D.settings.theme);
  const win=h('div',{class:'win framed'});let phase=D.confirm?'confirm':'run';
  const view=BootView({message:'',oncancel:()=>call('cancel')});
  const confirmV=h('div',{class:'boot'},h('div',{class:'topbar'}),h('img',{class:'ic',src:window.APP_ICON,alt:''}),h('div',{class:'sp'}),
    h('div',{class:'msg'},'Roblox wants to start a game. Launch it?'),h('div',{style:{display:'flex',gap:'8px',justifyContent:'center',marginTop:'8px',marginBottom:'16px'}},
      Btn('Launch',{kind:'primary',cls:'',onclick:()=>{call('start');swap(view);poll()}}),Btn(t('Common.Cancel'),{onclick:()=>call('cancel')})));
  const errV=h('div',{class:'boot'},h('div',{class:'topbar'}));
  const swap=v=>{win.innerHTML='';win.append(v)};
  swap(phase==='confirm'?confirmV:view);document.title=D.title||NAME;
  async function poll(){
    const s=await call('progress');if(!s)return setTimeout(poll,100);
    if(s.finished!=null&&s.error){errV.innerHTML='';errV.append(h('div',{class:'topbar'}),h('img',{class:'ic',src:window.APP_ICON,alt:''}),h('div',{class:'sp'}),
      h('div',{class:'msg',style:{color:'var(--crit)'}},'Something went wrong'),h('div',{class:'t3',style:{textAlign:'center',fontSize:'12px',marginBottom:'10px',maxHeight:'60px',overflow:'auto'}},s.error),
      h('div',{class:'pb err pbw',style:{marginBottom:'16px'}},h('div',{class:'v',style:{width:'100%'}})),Btn(t('Common.Close'),{cls:'',onclick:()=>call('win',{op:'close',force:true})}));
      errV.querySelector('.btn').style.cssText='width:120px;align-self:center';swap(errV);return}
    if(s.finished!=null){call('win',{op:'close',force:true});return}
    view.set(s);setTimeout(poll,60)}
  if(phase==='run'){call('start');poll()}
  root.append(win)}

/* =========================================================== HOME (launch menu) */
function Home(){
  window.__themeMode=D.settings.theme;applyTheme(D.settings.theme);
  const win=h('div',{class:'win framed'});
  const go=arg=>call('launch',{arg});
  const left=h('div',{style:{display:'flex',flexDirection:'column',justifyContent:'center',padding:'24px 0'}},
    h('div',{style:{display:'flex',alignItems:'center',justifyContent:'center',gap:'12px',marginBottom:'32px'}},h('img',{src:window.APP_ICON,style:{width:'64px',height:'64px'},alt:''}),
      h('div',null,h('div',{style:{fontSize:'24px'}},NAME),h('div',{class:'t3',style:{fontSize:'12px'}},t('Menu.About.Version',D.version)))),
    h('div',{style:{display:'flex',flexDirection:'column',alignItems:'center'}},
      Hyperlink('QuestionCircle48',t('About.Title'),()=>openUrl(D.links.repo)),Hyperlink('Book24',t('LaunchMenu.BloxstrapWiki'),WIKI),Hyperlink('PeopleCommunity28',t('LaunchMenu.RobloxCommunity'),D.links.repo)));
  const right=h('div',{style:{padding:'16px'}},
    CardAction({icon:'ArrowRight12',title:t('LaunchMenu.LaunchRoblox'),onclick:()=>go('-player'),style:{marginTop:0}}),
    CardAction({icon:'Wrench24',title:t('LaunchMenu.LaunchRobloxStudio'),onclick:()=>go('-studio')}),
    CardAction({icon:'Settings28',title:t('LaunchMenu.ConfigureSettings'),onclick:()=>go('-settings')}),
    h('div',{style:{height:'16px'}}),
    CardAction({icon:'BookQuestionMark24',title:t('LaunchMenu.Wiki.Title'),desc:t('LaunchMenu.Wiki.Description'),onclick:()=>openUrl(WIKI+'/help/')}));
  const upd=UpdateBar();
  wrap(win,TitleBar({title:NAME}),upd,h('div',{style:{display:'grid',gridTemplateColumns:'280px 1fr',flex:1,minHeight:0}},left,right));root.append(win);document.title=NAME;
  if(D.can_update&&D.settings.check_for_updates)upd.check(false)}

/* =========================================================== SETUP (installer) */
function Setup(){
  window.__themeMode='system';applyTheme(D.settings.theme||'system');
  const S={step:0,loc:D.default_location,desktop:true,start:true,err:'',busy:false,after:1};
  const win=h('div',{class:'win'});
  const STEPS=[t('Installer.Welcome.Title'),t('Installer.Install.Title'),t('Installer.Completion.Title')];
  const nav=h('div',{class:'nav',style:{width:'200px'}});const crumb=h('div',{class:'crumb'});const pageWrap=h('div',{class:'page-wrap'});
  const back=Btn(t('Common.Navigation.Back'),{onclick:()=>{S.step--;draw()}});back.style.width='100px';
  const next=Btn('',{kind:'primary',onclick:onNext});next.style.width='100px';
  const close=Btn(t('Common.Close'),{onclick:()=>onClose()});close.style.width='100px';
  const status=h('div',{class:'status'},h('div',{class:'l'}),back,h('span',{style:{width:'8px'}}),next,h('span',{style:{width:'12px'}}),close);
  async function onClose(){if(S.step===2||await confirmBox(t('Common.Warning'),t('Installer.ShouldCancel')))call('win',{op:'close',force:true})}
  async function onNext(){
    if(S.step===0){S.step=1;draw();return}
    if(S.step===1){
      if(!S.loc.trim()){S.err=t('Menu.InstallLocation.NotSet');draw();return}
      S.busy=true;S.err='';draw();
      try{await call('setup_install',{location:S.loc,desktop:S.desktop,start:S.start});S.busy=false;S.step=2}catch(e){S.busy=false;S.err=String(e)}
      draw();return}
    }
  function draw(){
    clearRegs();nav.innerHTML='';STEPS.forEach((s,i)=>nav.append(h('div',{class:'nav-item'+(i===S.step?' active':'')+(i<S.step?' done':'')},icon('ArrowCircleRight48'),h('span',null,s))));
    crumb.innerHTML='';crumb.append(h('span',null,STEPS[S.step]));pageWrap.innerHTML='';pageWrap.append([welcome,install,done][S.step]());
    back.disabled=S.step===0||S.step===2||S.busy;back.classList.toggle('dis',back.disabled);
    next.textContent=S.step===1?t('Common.Navigation.Install'):t('Common.Navigation.Next');next.style.visibility=S.step===2?'hidden':'visible';next.disabled=S.busy;next.classList.toggle('dis',S.busy);
    close.textContent=t('Common.Close');close.disabled=S.busy;close.classList.toggle('dis',S.busy)}
  const welcome=()=>h('div',{class:'page'},h('div',{style:{fontSize:'14px',lineHeight:1.4}},md('Installer.Welcome.MainText',D.links.repo,D.links.repo)),h('div',{style:{marginTop:'24px',fontSize:'14px'}},t('Installer.Welcome.NextToContinue')));
  const install=()=>{
    const loc=Text(()=>S.loc,v=>S.loc=v);
    return h('div',{class:'page'},h('div',{style:{fontSize:'20px',fontWeight:600}},t('Installer.Install.Location.Title')),h('div',{style:{fontSize:'14px'}},t('Installer.Install.Location.Text')),
      h('div',{class:'card p12'},h('div',{style:{display:'grid',gridTemplateColumns:'1fr auto auto',gap:'8px'}},loc,Btn(t('Common.Browse'),{icon:'Folder24',onclick:async()=>{const p=await call('browse_folder');if(p){S.loc=p;loc.input.value=p}}}),Btn(t('Common.Reset'),{icon:'ArrowCounterclockwise24',onclick:()=>{S.loc=D.default_location;loc.input.value=S.loc}}))),
      S.err?h('div',{style:{marginTop:'8px',color:'var(--crit)',fontSize:'14px',whiteSpace:'pre-wrap'}},S.err):null,
      h('div',{style:{fontSize:'20px',fontWeight:600,marginTop:'16px'}},t('Common.Shortcuts')),h('div',{style:{fontSize:'14px'}},t('Installer.Install.Shortcuts.Description')),
      h('div',{class:'grid2'},Option({header:t('Common.Shortcuts.Desktop'),content:Toggle(()=>S.desktop,v=>S.desktop=v)}),Option({header:t('Common.Shortcuts.StartMenu'),content:Toggle(()=>S.start,v=>S.start=v)})))};
  const done=()=>h('div',{class:'page'},h('div',{style:{fontSize:'14px',marginTop:'8px',lineHeight:1.4,whiteSpace:'pre-wrap'}},t('Installer.Completion.Text')),
    CardAction({icon:'Settings28',title:t('Installer.Completion.Settings.Title'),desc:t('Installer.Completion.Settings.Description'),style:{marginTop:'16px'},onclick:()=>call('setup_finish',{after:1})}),
    CardAction({icon:'ArrowRight12',title:t('Installer.Completion.Launch.Title'),desc:t('Installer.Completion.Launch.Description'),onclick:()=>call('setup_finish',{after:0})}));
  window.__tryClose=onClose;
  wrap(win,TitleBar({title:t('Installer.Title'),min:true,max:true,onclose:()=>onClose()}),
    h('div',{class:'root'},nav,h('div',{class:'main'},crumb,pageWrap)),status,ResizeHandles());root.append(win);document.title=t('Installer.Title');draw()}

/* =========================================================== boot */
const MODES={settings:Settings,home:Home,launch:Launch,setup:Setup};
(MODES[D.mode]||Settings)();
call('ready');
document.addEventListener('contextmenu',e=>{if(!/INPUT|TEXTAREA/.test(e.target.tagName))e.preventDefault()});
})();
