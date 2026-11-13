/* core: helpers, bridge, i18n, theme, controls */
(function(){
const NS='http://www.w3.org/2000/svg';
const EN=window.STRINGS||{};const ID=window.STRINGS_ID||{};let STR=EN;
/* language: texts missing from the Indonesian table fall back to English */
window.setLang=function(code){STR=code==='id'?Object.assign({},EN,ID):EN};
window.effLang=function(pref,sys){return pref==='id'||pref==='en'?pref:(sys==='id'?'id':'en')};
window.t=function(k,...a){let s=STR[k];if(s==null)s=k;return s.replace(/\{(\d+)\}/g,(m,i)=>a[i]!=null?a[i]:m)};
window.h=function(tag,props,...kids){
  const el=document.createElement(tag);
  if(props)for(const k in props){const v=props[k];if(v==null||v===false)continue;
    if(k==='class')el.className=v;else if(k==='html')el.innerHTML=v;else if(k==='style'&&typeof v==='object')Object.assign(el.style,v);
    else if(k.startsWith('on'))el.addEventListener(k.slice(2),v);else if(v===true)el.setAttribute(k,'');else el.setAttribute(k,v)}
  add(el,kids);return el};
function add(el,kids){for(const c of kids){if(c==null||c===false)continue;if(Array.isArray(c))add(el,c);else el.append(c.nodeType?c:document.createTextNode(String(c)))}}
window.icon=function(name,cls){
  const d=(window.ICONS||{})[name];const s=document.createElementNS(NS,'svg');s.setAttribute('class','i'+(cls?' '+cls:''));
  if(d){s.setAttribute('viewBox',d.v);s.innerHTML=d.p}else s.setAttribute('viewBox','0 0 24 24');return s};
/* tiny markdown: [text](url), **bold**, \n */
window.md=function(src,...args){
  let s=t(src,...args);if(s===src&&!STR[src])s=src;
  const el=h('span',{class:'md'});
  const re=/\[([^\]]+)\]\(([^)]+)\)/g;let last=0,m;
  const parts=s.split('\n');
  parts.forEach((line,li)=>{if(li)el.append(h('br'));let last=0,m;re.lastIndex=0;
    while((m=re.exec(line))){if(m.index>last)el.append(line.slice(last,m.index));
      const url=m[2];el.append(h('a',{onclick:()=>openUrl(url)},m[1]));last=m.index+m[0].length}
    if(last<line.length)el.append(line.slice(last))});
  return el};
window.fmt=(k,...a)=>t(k,...a);

/* ---- bridge ---- */
const pending={};let seq=1;
window.__reply=function(id,ok,val){const p=pending[id];if(!p)return;delete pending[id];ok?p.res(val):p.rej(val)};
window.call=function(cmd,args){
  return new Promise((res,rej)=>{
    if(window.ipc&&window.ipc.postMessage){const id=seq++;pending[id]={res,rej};window.ipc.postMessage(JSON.stringify({id,cmd,args:args||{}}))}
    else if(window.__mock){Promise.resolve(window.__mock(cmd,args||{})).then(res,rej)}
    else res(null)})};
window.openUrl=u=>call('open_url',{url:u});

/* ---- theme ---- */
const mq=window.matchMedia('(prefers-color-scheme: dark)');
window.applyTheme=function(mode){
  const dark=mode==='dark'||(mode!=='light'&&mq.matches);
  const r=document.documentElement;r.classList.toggle('dark',dark);
  if(CSS.supports('color','AccentColor')){r.style.setProperty('--accent',dark?'color-mix(in srgb,AccentColor 55%,#fff)':'AccentColor')}
};
mq.addEventListener('change',()=>{if(window.__themeMode==='system'||!window.__themeMode)applyTheme('system')});

/* ---- controls ---- */
window.Toggle=function(get,set,opts){
  opts=opts||{};const el=h('div',{class:'tg',role:'switch',tabindex:0});
  const sync=()=>{el.classList.toggle('on',!!get());el.setAttribute('aria-checked',!!get())};
  const flip=()=>{if(el.classList.contains('dis'))return;set(!get());sync();touch()};
  el.addEventListener('click',flip);el.addEventListener('keydown',e=>{if(e.key===' '||e.key==='Enter'){e.preventDefault();flip()}});
  sync();el.sync=sync;reg(sync);return el};
window.Check=function(label,get,set){
  const el=h('div',{class:'cb',tabindex:0},h('div',{class:'bx'},icon('Checkmark20')),h('span',null,label));
  const sync=()=>el.classList.toggle('on',!!get());
  el.addEventListener('click',()=>{set(!get());sync();touch()});sync();reg(sync);return el};
window.Btn=function(label,o){o=o||{};
  const el=h('button',{class:'btn'+(o.kind?' '+o.kind:'')+(o.cls?' '+o.cls:''),onclick:o.onclick},o.icon?icon(o.icon):null,label!=null?h('span',null,label):null);
  if(o.enabled){const f=()=>{el.disabled=!o.enabled();el.classList.toggle('dis',!o.enabled())};f();reg(f)}
  return el};
window.Combo=function(items,get,set,width){
  // items: [{v,label}]
  const lab=h('span');const el=h('div',{class:'cbx',style:{width:(width||200)+'px'}},h('div',{class:'sel',tabindex:0},lab,icon('ChevronDown20')));
  const sync=()=>{const it=items.find(i=>i.v===get())||items[0];lab.textContent=it?it.label:''};
  let pop=null;const close=()=>{if(pop){pop.remove();pop=null;el.classList.remove('open');document.removeEventListener('mousedown',outside,true)}};
  const outside=e=>{if(pop&&!pop.contains(e.target)&&!el.contains(e.target))close()};
  el.addEventListener('click',()=>{if(pop)return close();
    const r=el.getBoundingClientRect();pop=h('div',{class:'pop'});pop.style.minWidth=r.width+'px';
    const cur=get();items.forEach(i=>pop.append(h('div',{class:'it'+(i.v===cur?' sel':''),onclick:e=>{e.stopPropagation();set(i.v);sync();close();touch()}},i.label)));
    document.body.append(pop);const ph=pop.offsetHeight;let top=r.top-4;if(top+ph>innerHeight-8)top=Math.max(8,innerHeight-8-ph);
    pop.style.left=r.left+'px';pop.style.top=Math.max(8,top)+'px';el.classList.add('open');setTimeout(()=>document.addEventListener('mousedown',outside,true))});
  sync();reg(sync);return el};
window.Text=function(get,set,o){o=o||{};
  const inp=h(o.multi?'textarea':'input',{placeholder:o.placeholder||'',spellcheck:false,rows:o.rows||3,type:o.multi?null:(o.type||null),autocomplete:'off'});inp.value=get()||'';
  inp.addEventListener('input',()=>{set(inp.value);touch()});
  const el=h('div',{class:'tb'},o.icon?icon(o.icon):null,inp);
  reg(()=>{if(document.activeElement!==inp)inp.value=get()||''});el.input=inp;return el};
window.Card=function(...kids){return h('div',{class:'card'},kids)};
window.Option=function(o){
  // {header,desc(Node|string),help,enabled(),content}
  const hd=h('div',{class:'hd'},h('span',null,o.header));
  if(o.help)hd.append(h('span',{class:'help',title:t('Menu.MoreInfo'),onclick:()=>openUrl(o.help)},icon('QuestionCircle48')));
  const tx=h('div',{class:'tx'},hd);
  if(o.desc)tx.append(h('div',{class:'ds'},typeof o.desc==='string'?md(o.desc):o.desc));
  const el=h('div',{class:'card opt'},tx,o.content);
  if(o.enabled){const f=()=>{const on=o.enabled();el.classList.toggle('off',!on);if(o.content&&o.content.classList)o.content.classList.toggle('dis',!on)};f();reg(f)}
  return el};
window.CardAction=function(o){
  const tx=h('div',{class:'tx'},h('div',{class:'tt'},o.title));
  if(o.desc)tx.append(h('div',{class:'ds'},o.desc));
  const el=h('div',{class:'card act'+(o.desc?'':' p12'),tabindex:0,onclick:o.onclick,style:o.style},o.icon?icon(o.icon):null,tx,icon('ChevronRight20','chev'));
  el.addEventListener('keydown',e=>{if(e.key==='Enter'||e.key===' ')o.onclick()});return el};
window.Expander=function(o){
  const body=h('div',{class:'in'},o.body);
  const el=h('div',{class:'exp'+(o.open?' open':'')},
    h('div',{class:'hd',onclick:()=>el.classList.toggle('open')},o.icon?icon(o.icon):null,h('div',{class:'tx'},h('div',null,o.title),o.desc?h('div',{class:'ds t3',style:{fontSize:'12px',marginTop:'2px'}},o.desc):null),icon('ChevronDown20','chev')),
    h('div',{class:'bd'},h('div',null,body)));
  return el};
window.Hyperlink=function(ic,label,onclick){if(typeof onclick==='string'){const u=onclick;onclick=()=>openUrl(u)}return h('div',{class:'hl',tabindex:0,onclick},ic?icon(ic):null,h('span',null,label))};

/* ---- reactive registry (per view) ---- */
let regs=[];window.reg=f=>{regs.push(f)};window.clearRegs=()=>{regs=[]};
let touchCb=null;window.onTouch=f=>{touchCb=f};
window.touch=function(){regs.forEach(f=>{try{f()}catch(e){}});if(touchCb)touchCb()};

/* ---- snackbar & dialogs ---- */
window.snack=function(o){
  let box=document.querySelector('.snacks');if(!box){box=h('div',{class:'snacks'});document.body.append(box)}
  const el=h('div',{class:'snack '+(o.kind||'')},icon(o.icon||'Info20'),h('div',null,h('div',{class:'tt'},o.title),o.msg?h('div',{class:'ds'},o.msg):null),h('div',{class:'x',onclick:()=>kill()},icon('Dismiss12')));
  const kill=()=>{el.classList.add('out');setTimeout(()=>el.remove(),200)};box.append(el);setTimeout(kill,o.timeout||3000);return el};
window.dialog=function(o){
  // {title, body(Node|string), buttons:[{label,kind,value}], wide}
  return new Promise(res=>{
    const close=v=>{scrim.remove();res(v)};
    const body=h('div',{class:'db'+(o.html?' nw':'')},typeof o.body==='string'?o.body:o.body);
    const btns=(o.buttons||[{label:t('Common.OK'),kind:'primary',value:true}]).map(b=>Btn(b.label,{kind:b.kind,onclick:()=>close(b.value)}));
    const scrim=h('div',{class:'scrim',onmousedown:e=>{if(e.target===scrim&&o.dismiss!==false&&o.cancelValue!==undefined)close(o.cancelValue)}},
      h('div',{class:'dlg'+(o.wide?' wide':''),role:'dialog'},o.title?h('div',{class:'dh'},o.title):null,body,h('div',{class:'df'},btns)));
    document.body.append(scrim);setTimeout(()=>{const p=scrim.querySelector('.btn.primary')||scrim.querySelector('.btn');p&&p.focus()},30)})};
window.confirmBox=(title,body,yes,no)=>dialog({title,body,buttons:[{label:yes||t('Common.Yes'),kind:'primary',value:true},{label:no||t('Common.No'),value:false}],cancelValue:false});

/* ---- title bar & window chrome ---- */
window.TitleBar=function(o){
  const bar=h('div',{class:'titlebar'},
    h('div',{class:'drag',ondblclick:()=>o.max&&call('win',{op:'max'})},h('img',{src:window.APP_ICON,alt:''}),h('span',null,o.title)));
  if(o.min)bar.append(h('div',{class:'tb-btn',title:t('X.Win.Min'),onclick:()=>call('win',{op:'min'})},''));
  if(o.max){const m=h('div',{class:'tb-btn',title:t('X.Win.Max'),onclick:()=>call('win',{op:'max'})},'');bar.append(m);window.addEventListener('resize',()=>{m.textContent=(innerWidth>=screen.availWidth-16&&innerHeight>=screen.availHeight-16)?'':''})}
  if(o.close!==false)bar.append(h('div',{class:'tb-btn close',title:t('Common.Close'),onclick:o.onclose||(()=>call('win',{op:'close'}))},''));
  return bar};
window.ResizeHandles=function(){
  const f=document.createDocumentFragment();
  ['n','s','e','w','nw','ne','sw','se'].forEach(d=>f.append(h('div',{class:'resize rz-'+d,onmousedown:e=>{e.preventDefault();call('win',{op:'resize',dir:d})}})));return f};
})();
