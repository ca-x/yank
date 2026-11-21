"use strict";
const { msg, t, setText } = YankI18n;
const $ = s => document.querySelector(s);
const labels = {all:msg("nav.all"),pinned:msg("nav.pinned"),text:msg("kind.text"),link:msg("kind.link"),image:msg("kind.image"),files:msg("kind.files"),code:msg("kind.code"),color:msg("kind.color"),accounts:msg("nav.accounts"),devices:msg("nav.devices")};
const kindIcons = {text:"file-text",link:"link",image:"image",files:"folder",code:"code",color:"palette"};
function kindIcon(kind) {const icon=node("img","kubai-icon clip-kind");icon.setAttribute("src","/icons/"+(kindIcons[kind]||"clipboard")+".svg");icon.setAttribute("alt","");icon.setAttribute("aria-hidden","true");icon.setAttribute("width","28");icon.setAttribute("height","28");return icon;}
let token = "", currentUser = null;
try { localStorage.removeItem("yank.token"); } catch {}
try { sessionStorage.removeItem("yank.token"); } catch {}
let view = "all", clips = [], selected = null, generation = 0, currentAbort, more = false, busy = false;
const checkedClips = new Set();
let searchTimer, listPosition, countsNeeded=true;
let theme;
try { theme = localStorage.getItem("yank.appearance"); } catch {}
if (!["system", "light", "dark"].includes(theme)) theme="system";
let toastTimer, toastHideTimer;
function notify(message) {
  const toast=$("#toast");setText(toast,message);toast.classList.remove("closing");toast.hidden=false;
  clearTimeout(toastTimer);clearTimeout(toastHideTimer);
  toastTimer=setTimeout(()=>{toast.classList.add("closing");toastHideTimer=setTimeout(()=>toast.hidden=true,180);},3500);
}
function applyTheme() {
  document.documentElement.dataset.theme=theme==="system"?(matchMedia("(prefers-color-scheme: dark)").matches?"dark":"light"):theme;
  document.querySelectorAll("[data-theme-choice]").forEach(button=>button.setAttribute("aria-pressed",String(button.dataset.themeChoice===theme)));
}
applyTheme();matchMedia("(prefers-color-scheme: dark)").addEventListener("change",applyTheme);
document.querySelectorAll("[data-theme-choice]").forEach(button=>button.onclick=()=>{
  theme=button.dataset.themeChoice;try {localStorage.setItem("yank.appearance",theme);} catch {}applyTheme();
});
function uiError(key, params={}) { const error=new Error(t(key,params));error.localized=msg(key,params);return error; }
function errorMessage(error) { return error.localized || msg("request.network"); }
async function api(path, options={}) {
  let r;
  try { r=await fetch(path,{...options,headers:{"Content-Type":"application/json",...options.headers}}); }
  catch(error) { if(error.name==="AbortError")throw error;throw uiError("request.network"); }
  if(!r.ok){
    let key=({401:"auth.error",403:"request.forbidden",404:"request.missing",409:"request.conflict",429:"request.rate"})[r.status];
    if(!key)key=r.status>=500?"request.server":r.status===400?(path.includes("password")?"password.error":path==="/api/accounts"?"accounts.invalid":"request.invalid"):"request.error";
    throw uiError(key,{status:r.status});
  }
  if(r.status===204)return null;
  try { return await r.json(); } catch { throw uiError("request.server"); }
}
function openSettings(){
  $("#token").value="";
  $("#username").value=currentUser?.username||"";
  setText($("#settings-error"),"");
  setText($("#settings-title"),msg(currentUser?"settings.connected":"settings.title"));
  setText($("#settings-submit"),msg(currentUser?"settings.switch":"settings.submit"));
  $("#session-actions").hidden=!currentUser;
  $("#logout").hidden=!currentUser;
  $("#change-password").hidden=!currentUser;
  setText($("#session-identity"),currentUser?msg("status.signedIn",{username:currentUser.username}):"");
  $("#settings").showModal();
  (currentUser?$("#change-password"):$("#username")).focus();
}
["#connection","#connect-top","#empty-connect"].forEach(id=>$(id).onclick=openSettings);
$("#close-settings").onclick=()=>$("#settings").close();
$("#settings-form").onsubmit=async e=>{e.preventDefault();const submit=e.submitter;submit.disabled=true;try{const response=await api("/api/login",{method:"POST",body:JSON.stringify({username:$("#username").value,password:$("#token").value,remember:$("#remember").checked})});clearSession();currentUser=response.user;token="session";$("#token").value="";$("#accounts-nav").hidden=!currentUser.admin;$("#settings").close();await refresh();}catch(err){setText($("#settings-error"),errorMessage(err));}finally{submit.disabled=false;}};
function clearSession() {
  checkedClips.clear();
  clearTimeout(searchTimer);$("#library-layout").inert=false;countsNeeded=true;
  currentAbort?.abort(); generation++;
  currentUser=null; token=""; $("#search").value=""; clips=[]; selected=null; more=false; view="all";
  $("#mobile-view").value="all"; $("#page-description").hidden=false; $("#library-view").hidden=false; $("#accounts-view").hidden=true; $("#devices-view").hidden=true;
  $("#accounts-nav").hidden=true; $("#accounts").replaceChildren(); $("#devices").replaceChildren();
  setText($("#connection-status"),msg("status.signedOut")); $("#connection-dot").classList.remove("online");
  setText($("#connect-top"),msg("connect")); setText($("#breadcrumb"),labels.all);
  setText($("#page-title"),msg("library.title"));
  setText($("#nav-count"),"—"); setText($("#pin-count"),"—"); $("#refresh").disabled=false; $("#more").disabled=false;
  document.querySelectorAll("[data-view]").forEach(n=>{const active=n.dataset.view==="all";n.classList.toggle("selected",active);if(active)n.setAttribute("aria-current","page");else n.removeAttribute("aria-current");});
  render();
}
$("#logout").onclick=async()=>{try{await api("/api/logout",{method:"POST"});clearSession();$("#settings").close();notify(msg("logout.done"));}catch(e){notify(errorMessage(e));}};
function node(tag,cls,text){const el=document.createElement(tag);if(cls)el.className=cls;if(text!==undefined)setText(el,text);return el;}
function date(ts){return msg("date",{timestamp:ts});}
setInterval(()=>{
  document.querySelectorAll(".clip-time[data-timestamp]").forEach(element=>{
    setText(element,msg("relativeDate",{timestamp:Number(element.dataset.timestamp)}));
  });
},60000);
function select(id){
  const compact=matchMedia("(max-width:900px)").matches;
  if(!selected)listPosition=compact?{id,top:window.scrollY,width:window.innerWidth}:null;
  selected=clips.find(clip=>clip.id===id)||null;
  document.querySelectorAll(".clip-row").forEach(row=>{
    row.classList.toggle("selected",row.dataset.id===id);
    row.setAttribute("aria-pressed",String(row.dataset.id===id));
  });
  renderDetail();
  if(compact&&selected){
    $("#detail-title").focus({preventScroll:true});
    $("#inspector").scrollIntoView({block:"nearest"});
  }
}
function renderDetail(){const c=selected;$("#detail").hidden=!c;$("#close-detail").hidden=!c;$("#detail-empty").hidden=!!c;$("#inspector").classList.toggle("open",!!c);if(!c)return;setText($("#detail-kind"),labels[c.kind]||c.kind);setText($("#detail-title"),c.description||msg("record.untitled"));setText($("#detail-content"),c.text||msg("binary.hint"));setText($("#pin span"),c.pinned?msg("unpin"):msg("pin"));$("#copy").disabled=!c.text;$("#detail-meta").replaceChildren();for(const [k,v]of [[msg("meta.source"),c.device_id],[msg("meta.time"),date(c.updated_at)],[msg("meta.formats"),c.formats.map(f=>f.mime||f.format).join(" · ")],[msg("meta.collection"),c.collection||msg("meta.unfiled")]]){$("#detail-meta").append(node("dt","",k),node("dd","",v));}}
function renderSelection() {
  $("#select-all").checked=clips.length>0&&checkedClips.size===clips.length;
  $("#select-all").indeterminate=checkedClips.size>0&&!$("#select-all").checked;
  $("#list-heading").hidden=checkedClips.size>0;
  setText($("#selection-count"),checkedClips.size?msg("bulk.count",{count:checkedClips.size}):"");
  $("#delete-selected").hidden=!checkedClips.size;$("#clear-selection").hidden=!checkedClips.size;
}
function render(){
  const wrap=$("#clips");wrap.replaceChildren();let previousGroup;
  for(const clip of clips){
    const group=clip.pinned?"pinned":new Date(clip.updated_at*1000).toDateString();
    if(group!==previousGroup){wrap.append(node("div","day-heading",clip.pinned?msg("nav.pinned"):msg("dayGroup",{timestamp:clip.updated_at})));previousGroup=group;}
    const item=node("button","clip-row"+(selected?.id===clip.id?" selected":""));
    item.dataset.id=clip.id;item.setAttribute("aria-pressed",String(selected?.id===clip.id));
    if(clip.description)item.setAttribute("aria-label",clip.description);else{item.dataset.i18nAriaLabel="record.label";item.setAttribute("aria-label",t("record.label"));}
    const body=node("div","clip-body"),title=node("div","clip-title");
    title.append(node("span","",clip.description||msg("record.untitled")));
    if(clip.pinned)title.append(node("span","pin-marker",msg("record.pinned")));
    body.append(title,node("div","clip-snippet",clip.text.replace(/\s+/g," ").trim()||msg("record.formats",{count:clip.formats.length})));
    const time=node("div","clip-time",msg("relativeDate",{timestamp:clip.updated_at}));
    time.dataset.timestamp=clip.updated_at;
    const source=node("div","clip-source",clip.device_id);
    const origin=node("div","clip-origin");origin.append(source,time);
    item.append(kindIcon(clip.kind),body,origin);item.onclick=()=>select(clip.id);
    const row=node("div","selectable-row");row.setAttribute("role","listitem");
    const checkbox=node("input");checkbox.setAttribute("type","checkbox");checkbox.checked=checkedClips.has(clip.id);
    checkbox.dataset.i18nAriaLabel="bulk.record";checkbox.setAttribute("aria-label",t("bulk.record",{title:clip.description}));
    checkbox.dataset.recordTitle=clip.description;
    checkbox.onchange=()=>{if(checkbox.checked)checkedClips.add(clip.id);else checkedClips.delete(clip.id);renderSelection();};
    const checkTarget=node("label","row-check");checkTarget.append(checkbox);
    row.append(checkTarget,item);wrap.append(row);
  }
  renderSelection();
  const hasRecords=clips.length>0;
  $("#library-toolbar").hidden=!token;
  $("#refresh").hidden=!token;
  $("#bulk-actions").hidden=!hasRecords;
  $("#library-layout").classList.toggle("empty-library",!hasRecords);
  $("#export").disabled=!hasRecords;
  $("#more").hidden=!more;
  $("#empty").hidden=hasRecords;
  $("#empty-connect").hidden=!!token;
  const searching=!!$("#search").value;
  $("#clear-filter").hidden=!token||(!searching&&view==="all");
  setText($("#clear-filter"),msg(searching?"search.clear":"search.resetView"));
  const count=more?"records.countMore":clips.length===1?"records.single":"records.count";
  setText($("#result-count"),token?msg(count,{count:clips.length}):msg("records.connect"));
  if(!hasRecords){
    const title=!token?"empty.connect.title":searching?"empty.search.title":view==="all"?"empty.library.title":"search.filtered.title";
    const description=!token?"empty.connected":searching?"empty.search.description":view==="all"?"empty.library.description":"search.filtered.description";
    setText($("#empty h2"),msg(title));
    setText($("#empty p"),msg(description));
  }
  renderDetail();
}
async function refresh(append=false, updateCounts=true) {
  clearTimeout(searchTimer);
  if(!token){openSettings();return;}
  const gen=++generation;
  currentAbort?.abort();
  currentAbort=new AbortController();
  const signal=currentAbort.signal;
  const filtering=$("#library-layout").inert;
  const fromMore=append&&document.activeElement===$("#more");
  $("#refresh").disabled=true;
  $("#more").disabled=true;
  $("#refresh").hidden=false;
  const library=view!=="devices"&&view!=="accounts";
  if(library&&!clips.length){
    $("#library-toolbar").hidden=false;
    setText($("#empty h2"),msg("records.loading"));
    setText($("#empty p"),"");
    $("#clear-filter").hidden=true;
    $("#empty-connect").hidden=true;
    $("#empty").hidden=false;
  }
  try{
    if(view==="accounts"){
      await loadAccounts(signal,gen);
    }else if(view==="devices"){
      const peers=await api("/api/devices",{signal});
      if(gen!==generation)return;
      $("#devices").replaceChildren();
      for(const peer of peers){
        const card=node("article","device-card");
        card.append(node("span","tag",msg("devices.waiting")),node("h3","",peer.name),
          node("code","",peer.code),node("p","",msg("devices.identity")));
        $("#devices").append(card);
      }
      if(!peers.length)$("#devices").append(node("p","muted",msg("devices.empty")));
    }else{
      const query=encodeURIComponent($("#search").value);
      const offset=append?clips.length:0;
      const data=await api(`/api/clips?q=${query}&filter=${encodeURIComponent(view)}&offset=${offset}`,{signal});
      if(gen!==generation)return;
      const focusId=document.activeElement?.dataset.id;
      $("#library-layout").inert=false;
      more=data.length===100;
      clips=append?[...clips,...data]:data;
      const available=new Set(clips.map(clip=>clip.id));
      for(const id of checkedClips)if(!available.has(id))checkedClips.delete(id);
      if(selected)selected=clips.find(clip=>clip.id===selected.id)||null;
      render();
      const nextId=fromMore&&(!document.activeElement||document.activeElement===document.body||document.activeElement===$("#more"))?data[0]?.id:focusId;
      if(nextId)document.querySelector(`.clip-row[data-id="${CSS.escape(nextId)}"]`)?.focus();
      if(updateCounts||countsNeeded){
        const stats=await api("/api/stats",{signal});
        if(gen!==generation)return;
        setText($("#nav-count"),stats.clips);
        setText($("#pin-count"),stats.pinned);
        countsNeeded=false;
      }
    }
    if(gen!==generation||!currentUser)return;
    setText($("#connection-status"),msg("status.signedIn",{username:currentUser.username}));
    $("#connection-dot").classList.add("online");
    setText($("#connect-top"),msg("settings.connected"));
  }catch(error){
    if(gen!==generation||error.name==="AbortError")return;
    if(error.localized?.key==="auth.error"){
      clearSession();
    }else{
      setText($("#connection-status"),msg("status.failed"));
      $("#connection-dot").classList.remove("online");
      if(filtering&&library){clips=[];more=false;render();}
      if(library&&!clips.length){
        $("#clear-filter").hidden=true;
        setText($("#empty h2"),msg("status.failed"));
        setText($("#empty p"),errorMessage(error));
      }
    }
    notify(errorMessage(error));
  }finally{
    if(gen===generation){$("#library-layout").inert=false;$("#refresh").disabled=false;$("#more").disabled=false;}
  }
}
$("#refresh").onclick=()=>refresh();
$("#more").onclick=()=>refresh(true,false);
$("#search").oninput=()=>{
  checkedClips.clear();
  selected=null;
  currentAbort?.abort();
  generation++;
  clearTimeout(searchTimer);
  $("#library-layout").inert=true;
  $("#export").disabled=true;
  setText($("#result-count"),msg("records.loading"));
  searchTimer=setTimeout(()=>refresh(false,false),150);
};
function showView(nextView) {
  if(!token){openSettings();return;}
  view=nextView;
  $("#library-layout").inert=false;
  checkedClips.clear();
  selected=null;
  clips=[];
  more=false;
  const library=view!=="devices"&&view!=="accounts";
  document.querySelectorAll("[data-view]").forEach(button=>{
    const active=button.dataset.view===view;
    button.classList.toggle("selected",active);
    if(active)button.setAttribute("aria-current","page");
    else button.removeAttribute("aria-current");
  });
  if(library)$("#mobile-view").value=view;
  setText($("#breadcrumb"),labels[view]);
  setText($("#page-title"),view==="all"?msg("library.title"):labels[view]);
  $("#page-description").hidden=!library;
  $("#accounts-view").hidden=view!=="accounts";
  $("#devices-view").hidden=view!=="devices";
  $("#library-view").hidden=!library;
  render();
  refresh(false,false);
}
document.querySelectorAll("[data-view]").forEach(button=>button.onclick=()=>showView(button.dataset.view));
$("#mobile-view").onchange=event=>showView(event.target.value);
$("#clear-filter").onclick=()=>{
  if($("#search").value){
    $("#search").value="";
    $("#search").focus();
    refresh(false,false);
  }else{
    showView("all");
    $("#search").focus();
  }
};
function closeDetail() {
  if(!selected)return;
  const id=selected.id; selected=null; renderDetail();
  document.querySelectorAll(".clip-row").forEach(row=>{row.classList.remove("selected");row.setAttribute("aria-pressed","false");});
  const row=document.querySelector(`.clip-row[data-id="${CSS.escape(id)}"]`);
  if(row&&listPosition?.id===id&&listPosition.width===window.innerWidth){
    row.focus({preventScroll:true});
    window.scrollTo({top:listPosition.top});
    row.scrollIntoView({block:"nearest"});
  }else{
    row?.focus();
  }
  listPosition=null;
}
$("#close-detail").onclick=closeDetail;
async function copy(){if(!selected?.text)return;try{await navigator.clipboard.writeText(selected.text);notify(msg("copy.done"));}catch{notify(msg("copy.error"));}}
$("#copy").onclick=copy;
async function mutation(path,method){
  if(!selected||busy)return;
  const owner=currentUser?.id,id=selected.id;
  const position=clips.findIndex(clip=>clip.id===id);
  busy=true;
  try{
    await api(`/api/clips/${encodeURIComponent(id)}${path}`,{method});
    if(owner!==currentUser?.id)return;
    const returnToList=selected?.id===id&&
      (method==="DELETE"||(view==="pinned"&&selected.pinned));
    countsNeeded=true;
    if(method==="DELETE"){
      if(selected?.id===id)selected=null;
      notify(msg("delete.done"));
    }
    await refresh();
    if(returnToList&&owner===currentUser?.id&&!selected&&!$("#library-view").hidden){
      const next=clips[Math.min(position,clips.length-1)];
      const target=next?document.querySelector(`.clip-row[data-id="${CSS.escape(next.id)}"]`):$("#search");
      target?.focus();
    }
  }catch(error){if(owner===currentUser?.id)notify(errorMessage(error));}
  finally{busy=false;}
}
$("#pin").onclick=()=>mutation("/pin","POST");$("#delete").onclick=()=>{if(confirm(msg("delete.confirm")))mutation("","DELETE");};
$("#export").onclick=()=>{const url=URL.createObjectURL(new Blob([JSON.stringify(clips,null,2)],{type:"application/json"}));const a=node("a");a.href=url;a.download="yank-page.json";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);notify(msg("export.done",{count:clips.length}));};
$("#export-all").onclick=async()=>{
  const button=$("#export-all"),owner=currentUser?.id;if(!owner)return;
  button.disabled=true;
  try{
    const backup=await api("/api/export");if(owner!==currentUser?.id)return;
    const url=URL.createObjectURL(new Blob([JSON.stringify(backup,null,2)],{type:"application/json"}));
    const anchor=node("a");anchor.href=url;anchor.download="yank-backup.json";anchor.click();
    setTimeout(()=>URL.revokeObjectURL(url),1000);notify(msg("export.allDone"));
  }catch(error){notify(errorMessage(error));}finally{button.disabled=false;}
};
$("#select-all").onchange=event=>{
  if(event.target.checked)clips.forEach(clip=>checkedClips.add(clip.id));
  else checkedClips.clear();
  render();
};
$("#clear-selection").onclick=()=>{checkedClips.clear();render();$("#select-all").focus();};
$("#delete-selected").onclick=async()=>{
  if(busy||!checkedClips.size||!confirm(t("bulk.confirm",{count:checkedClips.size})))return;
  const owner=currentUser?.id;if(!owner)return;busy=true;let removed=0;
  try{
    for(const id of [...checkedClips]){
      if(currentUser?.id!==owner)break;
      await api(`/api/clips/${encodeURIComponent(id)}`,{method:"DELETE"});
      checkedClips.delete(id);removed++;countsNeeded=true;
    }
    if(currentUser?.id===owner)notify(msg("bulk.done",{count:removed}));
  }catch(error){notify(msg("bulk.partial",{count:removed}));}
  finally{busy=false;if(currentUser?.id===owner){await refresh();if(!checkedClips.size&&view!=="devices"&&view!=="accounts"&&document.activeElement===document.body)(clips.length?$("#select-all"):$("#search")).focus();}}
};
document.addEventListener("pointerdown",()=>{document.documentElement.dataset.pointer="";});
document.addEventListener("keydown",event=>{
  delete document.documentElement.dataset.pointer;
  if(!token||$("#library-layout").inert||$("#library-view").hidden||$("#settings").open||$("#password-dialog").open||event.isComposing||event.ctrlKey||event.metaKey||event.altKey)return;
  if(event.key==="Escape"&&selected){closeDetail();return;}
  if(event.target.closest("input,textarea,select"))return;
  if(event.key==="/"){
    event.preventDefault();
    if(selected&&matchMedia("(max-width:900px)").matches)closeDetail();
    $("#search").focus();return;
  }
  const interactive=event.target.closest("button,a,[role=button]");
  if(interactive&&!interactive.classList.contains("clip-row"))return;
  if(event.key==="Enter"&&interactive){
    event.preventDefault();select(interactive.dataset.id);copy();return;
  }
  if(!["ArrowDown","ArrowUp"].includes(event.key)||!clips.length)return;
  event.preventDefault();
  const current=clips.findIndex(clip=>clip.id===(interactive?.dataset.id||selected?.id));
  const index=Math.max(0,Math.min(clips.length-1,current+(event.key==="ArrowDown"?1:-1)));
  const id=clips[index].id;
  // In the single-pane layout, arrows move through the list and Enter copies.
  if(!matchMedia("(max-width:900px)").matches||selected)select(id);
  if(!matchMedia("(max-width:900px)").matches||!selected){
    document.querySelector(`.clip-row[data-id="${CSS.escape(id)}"]`)?.focus();
  }
});
async function loadAccounts(signal,gen=generation){const owner=currentUser?.id;const users=await api("/api/accounts",{signal});if(gen!==generation||!owner||currentUser?.id!==owner)return;$("#accounts").replaceChildren();for(const user of users){const card=node("article","device-card");card.append(node("span","tag",user.admin?msg("accounts.admin"):msg("accounts.user")),node("h3","",user.username));if(user.id!==currentUser?.id){const button=node("button","button danger",msg("accounts.delete"));button.onclick=async()=>{if(!confirm(msg("accounts.deleteConfirm",{username:user.username})))return;try{await api(`/api/accounts/${encodeURIComponent(user.id)}`,{method:"DELETE"});await loadAccounts();}catch(e){notify(errorMessage(e));}};const reset=node("button","button secondary",msg("accounts.reset"));reset.onclick=()=>openPassword(user);const actions=node("div","account-actions");actions.append(reset,button);card.append(actions);}$("#accounts").append(card);}}
$("#create-account").onsubmit=async e=>{e.preventDefault();const button=e.submitter;button.disabled=true;setText($("#account-error"),"");try{await api("/api/accounts",{method:"POST",body:JSON.stringify({username:$("#new-username").value,password:$("#new-password").value,admin:$("#new-admin").checked})});$("#create-account").reset();await loadAccounts();notify(msg("accounts.created"));}catch(err){setText($("#account-error"),errorMessage(err));}finally{button.disabled=false;}};
render();
api("/api/session").then(user=>{currentUser=user;token="session";$("#accounts-nav").hidden=!user.admin;refresh();}).catch(()=>{});

let passwordTarget = null;
function openPassword(user) {
  passwordTarget = user.id === currentUser?.id ? null : user;
  $("#settings").close();
  $("#password-form").reset();
  setText($("#password-error"),"");
  setText($("#password-title"),passwordTarget ? msg("password.resetTitle",{username:user.username}) : msg("password.change"));
  $("#old-password-label").hidden = !!passwordTarget;
  $("#old-password").hidden = !!passwordTarget;
  $("#old-password").required = !passwordTarget;
  $("#password-dialog").showModal();
  (passwordTarget?$("#replacement-password"):$("#old-password")).focus();
}
$("#change-password").onclick = () => openPassword(currentUser);
$("#close-password").onclick = () => $("#password-dialog").close();
$("#password-form").onsubmit = async e => {
  e.preventDefault();
  const button = e.submitter;
  button.disabled = true;
  setText($("#password-error"),"");
  try {
    await api(passwordTarget ? `/api/accounts/${encodeURIComponent(passwordTarget.id)}/password` : "/api/password", {
      method: "POST", body: JSON.stringify({ old_password: $("#old-password").value, new_password: $("#replacement-password").value })
    });
    $("#password-form").reset();
    $("#password-dialog").close();
    if (!passwordTarget) {
      clearSession(); openSettings();
    }
    notify(msg("password.updated"));
  } catch (err) { setText($("#password-error"),errorMessage(err)); }
  finally { button.disabled = false; }
};

for (const id of ["#replacement-password", "#new-password"]) {
  const input = $(id);
  input.oninput = () => {
    const bytes = new TextEncoder().encode(input.value).length;
    input.setCustomValidity(bytes >= 8 && bytes <= 1024 ? "" : t("password.invalid"));
  };
}

// Locale changes only update text and attributes. No refresh, dialog reset or DOM replacement.
YankI18n.subscribe(() => {
  applyTheme();
  document.querySelectorAll("[data-record-title]").forEach(checkbox=>checkbox.setAttribute("aria-label",t("bulk.record",{title:checkbox.dataset.recordTitle})));
  for (const id of ["#replacement-password", "#new-password"]) $(id).oninput();
});
