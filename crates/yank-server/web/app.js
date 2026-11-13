"use strict";
const { msg, t, setText } = YankI18n;
const $ = s => document.querySelector(s);
const labels = {all:msg("nav.all"),pinned:msg("nav.pinned"),text:msg("kind.text"),link:msg("kind.link"),image:msg("kind.image"),files:msg("kind.files"),code:msg("kind.code"),color:msg("kind.color"),devices:msg("nav.devices")};
const kindIcons = {text:"file-text",link:"link",image:"image",files:"folder",code:"code",color:"palette"};
function kindIcon(kind) {const icon=node("img","kubai-icon clip-kind");icon.setAttribute("src","/icons/"+(kindIcons[kind]||"clipboard")+".svg");icon.setAttribute("alt","");icon.setAttribute("aria-hidden","true");icon.setAttribute("width","28");icon.setAttribute("height","28");return icon;}
let token = "", currentUser = null;
try { localStorage.removeItem("yank.token"); } catch {}
try { sessionStorage.removeItem("yank.token"); } catch {}
let view = "all", clips = [], selected = null, generation = 0, currentAbort, more = false, busy = false;
let theme;
try { theme = localStorage.getItem("yank.appearance"); } catch {}
if (!["system", "light", "dark"].includes(theme)) theme="system";
let toastTimer;
function notify(message) { setText($("#toast"),message);$("#toast").hidden=false;clearTimeout(toastTimer);toastTimer=setTimeout(()=>$("#toast").hidden=true,3500); }
function applyTheme() { document.documentElement.dataset.theme=theme==="system"?(matchMedia("(prefers-color-scheme: dark)").matches?"dark":"light"):theme;setText($("#theme"),msg("theme.value",{name:t("theme."+theme)})); }
applyTheme();matchMedia("(prefers-color-scheme: dark)").addEventListener("change",applyTheme);
$("#theme").onclick=()=>{theme={system:"light",light:"dark",dark:"system"}[theme];try { localStorage.setItem("yank.appearance",theme); } catch {}applyTheme();};
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
function openSettings(){ $("#token").value="";$("#username").value=currentUser?.username||"";setText($("#settings-error"),"");$("#logout").hidden=!currentUser;$("#change-password").hidden=!currentUser;$("#settings").showModal(); }
["#connection","#connect-top","#empty-connect"].forEach(id=>$(id).onclick=openSettings);
$("#close-settings").onclick=()=>$("#settings").close();
$("#settings-form").onsubmit=async e=>{e.preventDefault();const submit=e.submitter;submit.disabled=true;try{const response=await api("/api/login",{method:"POST",body:JSON.stringify({username:$("#username").value,password:$("#token").value,remember:$("#remember").checked})});currentUser=response.user;token="session";$("#token").value="";$("#accounts-nav").hidden=!currentUser.admin;$("#settings").close();await refresh();}catch(err){setText($("#settings-error"),errorMessage(err));}finally{submit.disabled=false;}};
function clearSession() {
  currentAbort?.abort(); generation++;
  currentUser=null; token=""; clips=[]; selected=null; more=false; view="all";
  $("#library-view").hidden=false; $("#accounts-view").hidden=true; $("#devices-view").hidden=true;
  $("#accounts-nav").hidden=true; $("#accounts").replaceChildren(); $("#devices").replaceChildren();
  setText($("#connection-status"),msg("status.signedOut")); $("#connection-dot").classList.remove("online");
  setText($("#connect-top"),msg("connect")); setText($("#breadcrumb"),labels.all);
  setText($("#page-title"),msg("library.title"));
  setText($("#nav-count"),"—"); setText($("#pin-count"),"—"); $("#refresh").disabled=false;
  document.querySelectorAll("[data-view]").forEach(n=>{const active=n.dataset.view==="all";n.classList.toggle("selected",active);if(active)n.setAttribute("aria-current","page");else n.removeAttribute("aria-current");});
  render();
}
$("#logout").onclick=async()=>{try{await api("/api/logout",{method:"POST"});clearSession();$("#settings").close();notify(msg("logout.done"));}catch(e){notify(errorMessage(e));}};
function node(tag,cls,text){const el=document.createElement(tag);if(cls)el.className=cls;if(text!==undefined)setText(el,text);return el;}
function date(ts){return msg("date",{timestamp:ts});}
function select(id){selected=clips.find(c=>c.id===id)||null;document.querySelectorAll(".clip-row").forEach(el=>{el.classList.toggle("selected",el.dataset.id===id);el.setAttribute("aria-pressed",String(el.dataset.id===id));});renderDetail();if(matchMedia("(max-width:900px)").matches&&selected){$("#detail-title").focus();$("#inspector").scrollIntoView({block:"start"});}}
function renderDetail(){const c=selected;$("#detail").hidden=!c;$("#detail-empty").hidden=!!c;$("#inspector").classList.toggle("open",!!c);if(!c)return;setText($("#detail-kind"),labels[c.kind]||c.kind);setText($("#detail-title"),c.description);setText($("#detail-content"),c.text||msg("binary.hint"));setText($("#pin"),c.pinned?msg("unpin"):msg("pin"));$("#copy").disabled=!c.text;$("#detail-meta").replaceChildren();for(const [k,v]of [[msg("meta.source"),c.device_id],[msg("meta.time"),date(c.updated_at)],[msg("meta.formats"),c.formats.map(f=>f.mime||f.format).join(" · ")],[msg("meta.collection"),c.collection||msg("meta.unfiled")]]){$("#detail-meta").append(node("dt","",k),node("dd","",v));}}
function render(){const wrap=$("#clips");wrap.replaceChildren();for(const c of clips){const item=node("button","clip-row"+(selected?.id===c.id?" selected":""));item.dataset.id=c.id;item.setAttribute("aria-pressed",String(selected?.id===c.id));if(c.description)item.setAttribute("aria-label",c.description);else {item.dataset.i18nAriaLabel="record.label";item.setAttribute("aria-label",t("record.label"));}const body=node("div","clip-body"),title=node("div","clip-title");title.append(node("span","",c.description||msg("record.untitled")));if(c.pinned)title.append(node("span","pin-marker",msg("record.pinned")));body.append(title,node("div","clip-snippet",c.text||msg("record.formats",{count:c.formats.length})));item.append(kindIcon(c.kind),body,node("div","clip-time",date(c.updated_at)));item.onclick=()=>select(c.id);const row=node("div");row.setAttribute("role","listitem");row.append(item);wrap.append(row);}setText($("#result-count"),msg(more?"records.countMore":"records.count",{count:clips.length}));$("#empty").hidden=clips.length>0;$("#more").hidden=!more;if(!token){setText($("#empty h2"),msg("empty.connect.title"));setText($("#empty p"),msg("empty.connected"));$("#empty-connect").hidden=false;setText($("#result-count"),msg("records.connect"));}if(!clips.length&&token){setText($("#empty h2"),$("#search").value?msg("empty.search.title"):msg("empty.library.title"));setText($("#empty p"),$("#search").value?msg("empty.search.description"):msg("empty.library.description"));$("#empty-connect").hidden=true;}renderDetail();}
async function refresh(append=false){if(!token){openSettings();return;}const gen=++generation;currentAbort?.abort();currentAbort=new AbortController();const signal=currentAbort.signal;$("#refresh").disabled=true;try{if(view==="accounts"){await loadAccounts(signal,gen);if(gen!==generation||!currentUser)return;}else if(view==="devices"){const peers=await api("/api/devices",{signal});if(gen!==generation)return;$("#devices").replaceChildren();for(const peer of peers){const card=node("article","device-card");card.append(node("span","tag",msg("devices.waiting")),node("h3","",peer.name),node("code","",peer.code),node("p","",msg("devices.identity")));$("#devices").append(card);}if(!peers.length)$("#devices").append(node("p","muted",msg("devices.empty")));}else{const data=await api(`/api/clips?q=${encodeURIComponent($("#search").value)}&filter=${encodeURIComponent(view)}&offset=${append?clips.length:0}`,{signal});if(gen!==generation)return;more=data.length===100;clips=append?[...clips,...data]:data;if(selected)selected=clips.find(c=>c.id===selected.id)||null;render();const stats=await api("/api/stats",{signal});if(gen!==generation)return;setText($("#nav-count"),stats.clips);setText($("#pin-count"),stats.pinned);}if(gen!==generation||!currentUser)return;setText($("#connection-status"),msg("status.signedIn",{username:currentUser?.username||""}));$("#connection-dot").classList.add("online");setText($("#connect-top"),msg("settings.connected"));}catch(err){if(gen===generation&&err.name!=="AbortError"){notify(errorMessage(err));setText($("#connection-status"),msg("status.failed"));$("#connection-dot").classList.remove("online");}}finally{if(gen===generation)$("#refresh").disabled=false;}}
$("#refresh").onclick=()=>refresh();$("#more").onclick=()=>refresh(true);$("#search").oninput=()=>refresh();
document.querySelectorAll("[data-view]").forEach(b=>b.onclick=()=>{view=b.dataset.view;selected=null;document.querySelectorAll("[data-view]").forEach(n=>{n.classList.toggle("selected",n===b);if(n===b)n.setAttribute("aria-current","page");else n.removeAttribute("aria-current");});setText($("#breadcrumb"),labels[view]);setText($("#page-title"),view==="all"?msg("library.title"):labels[view]);$("#accounts-view").hidden=true;$("#library-view").hidden=view==="devices";$("#devices-view").hidden=view!=="devices";refresh();});
function closeDetail() {
  const id=selected?.id; selected=null; renderDetail();
  document.querySelectorAll(".clip-row").forEach(row=>{row.classList.remove("selected");row.setAttribute("aria-pressed","false");});
  document.querySelector(`.clip-row[data-id="${CSS.escape(id||"")}"]`)?.focus();
}
$("#close-detail").onclick=closeDetail;
async function copy(){if(!selected?.text)return;try{await navigator.clipboard.writeText(selected.text);notify(msg("copy.done"));}catch{notify(msg("copy.error"));}}
$("#copy").onclick=copy;
async function mutation(path,method){if(!selected||busy)return;busy=true;try{await api(`/api/clips/${encodeURIComponent(selected.id)}${path}`,{method});if(method==="DELETE"){selected=null;notify(msg("delete.done"));}await refresh();}catch(err){notify(errorMessage(err));}finally{busy=false;}}
$("#pin").onclick=()=>mutation("/pin","POST");$("#delete").onclick=()=>{if(confirm(msg("delete.confirm")))mutation("","DELETE");};
$("#export").onclick=()=>{const url=URL.createObjectURL(new Blob([JSON.stringify(clips,null,2)],{type:"application/json"}));const a=node("a");a.href=url;a.download="yank-page.json";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);notify(msg("export.done",{count:clips.length}));};
document.addEventListener("keydown",e=>{if($("#settings").open||$("#password-dialog").open)return;if(e.key==="Escape"){closeDetail();return;}if(e.target.closest("input,textarea,select"))return;const interactive=e.target.closest("button,a,[role=button]");if(interactive&&!interactive.classList.contains("clip-row"))return;if(e.key==="/"){e.preventDefault();$("#search").focus();}if(e.key==="Enter"&&selected&&interactive?.classList.contains("clip-row")){e.preventDefault();select(interactive.dataset.id);copy();}if(["ArrowDown","ArrowUp"].includes(e.key)&&clips.length){e.preventDefault();let i=clips.findIndex(c=>c.id===selected?.id);i=Math.max(0,Math.min(clips.length-1,i+(e.key==="ArrowDown"?1:-1)));select(clips[i].id);document.querySelector(`.clip-row[data-id="${CSS.escape(clips[i].id)}"]`)?.focus();}});
$("#accounts-nav").onclick=()=>{view="accounts";$("#library-view").hidden=true;$("#devices-view").hidden=true;$("#accounts-view").hidden=false;setText($("#breadcrumb"),msg("nav.accounts"));setText($("#page-title"),msg("accounts.title"));refresh();};
async function loadAccounts(signal,gen=generation){const owner=currentUser?.id;const users=await api("/api/accounts",{signal});if(gen!==generation||!owner||currentUser?.id!==owner)return;$("#accounts").replaceChildren();for(const user of users){const card=node("article","device-card");card.append(node("span","tag",user.admin?msg("accounts.admin"):msg("accounts.user")),node("h3","",user.username));if(user.id!==currentUser?.id){const button=node("button","button danger",msg("accounts.delete"));button.onclick=async()=>{if(!confirm(msg("accounts.deleteConfirm",{username:user.username})))return;try{await api(`/api/accounts/${encodeURIComponent(user.id)}`,{method:"DELETE"});await loadAccounts();}catch(e){notify(errorMessage(e));}};const reset=node("button","button secondary",msg("accounts.reset"));reset.onclick=()=>openPassword(user);card.append(reset,button);}$("#accounts").append(card);}}
$("#create-account").onsubmit=async e=>{e.preventDefault();const button=e.submitter;button.disabled=true;setText($("#account-error"),"");try{await api("/api/accounts",{method:"POST",body:JSON.stringify({username:$("#new-username").value,password:$("#new-password").value,admin:$("#new-admin").checked})});$("#create-account").reset();await loadAccounts();notify(msg("accounts.created"));}catch(err){setText($("#account-error"),errorMessage(err));}finally{button.disabled=false;}};
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
  for (const id of ["#replacement-password", "#new-password"]) $(id).oninput();
});
