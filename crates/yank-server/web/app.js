"use strict";
const $ = s => document.querySelector(s);
const labels = {all:"全部记录",pinned:"收藏",text:"文字",link:"链接",image:"图片",files:"文件",devices:"设备与中继"};
const glyphs = {text:"T",link:"↗",image:"▧",files:"▤"};
let token = "", currentUser = null;
localStorage.removeItem("yank.token");sessionStorage.removeItem("yank.token");
let view = "all", clips = [], selected = null, generation = 0, currentAbort, more = false, busy = false;
let theme = localStorage.getItem("yank.appearance") || "system";
let toastTimer;
function notify(message) { $("#toast").textContent=message;$("#toast").hidden=false;clearTimeout(toastTimer);toastTimer=setTimeout(()=>$("#toast").hidden=true,3500); }
function applyTheme() { document.documentElement.dataset.theme=theme==="system"?(matchMedia("(prefers-color-scheme: dark)").matches?"dark":"light"):theme;$("#theme").textContent="外观 · "+({system:"自动",light:"浅色",dark:"深色"}[theme]); }
applyTheme();matchMedia("(prefers-color-scheme: dark)").addEventListener("change",applyTheme);
$("#theme").onclick=()=>{theme={system:"light",light:"dark",dark:"system"}[theme];localStorage.setItem("yank.appearance",theme);applyTheme();};
async function api(path, options={}) { const r=await fetch(path,{...options,headers:{"Content-Type":"application/json",...options.headers}});if(!r.ok){let message=r.status===401?"请检查用户名和密码，或重新登录。":`请求失败 (${r.status})`;try{const e=await r.json();if(r.status!==401)message=e.error||message;}catch{}throw new Error(message);}return r.status===204?null:r.json(); }
function openSettings(){ $("#token").value="";$("#username").value=currentUser?.username||"";$("#settings-error").textContent="";$("#logout").hidden=!currentUser;$("#change-password").hidden=!currentUser;$("#settings").showModal(); }
["#connection","#connect-top","#empty-connect"].forEach(id=>$(id).onclick=openSettings);
$("#close-settings").onclick=()=>$("#settings").close();
$("#settings-form").onsubmit=async e=>{e.preventDefault();const submit=e.submitter;submit.disabled=true;try{const response=await api("/api/login",{method:"POST",body:JSON.stringify({username:$("#username").value,password:$("#token").value,remember:$("#remember").checked})});currentUser=response.user;token="session";$("#token").value="";$("#accounts-nav").hidden=!currentUser.admin;$("#settings").close();await refresh();}catch(err){$("#settings-error").textContent=err.message;}finally{submit.disabled=false;}};
function clearSession() {
  currentAbort?.abort(); generation++;
  currentUser=null; token=""; clips=[]; selected=null; more=false; view="all";
  $("#library-view").hidden=false; $("#accounts-view").hidden=true; $("#devices-view").hidden=true;
  $("#accounts-nav").hidden=true; $("#accounts").replaceChildren(); $("#devices").replaceChildren();
  $("#connection-status").textContent="尚未登录"; $("#connection-dot").classList.remove("online");
  $("#connect-top").textContent="连接服务器"; $("#breadcrumb").textContent=labels.all;
  $("#page-title").textContent="随时找回，接着使用。";
  $("#nav-count").textContent="—"; $("#pin-count").textContent="—"; $("#refresh").disabled=false;
  document.querySelectorAll("[data-view]").forEach(n=>{const active=n.dataset.view==="all";n.classList.toggle("selected",active);if(active)n.setAttribute("aria-current","page");else n.removeAttribute("aria-current");});
  render();
}
$("#logout").onclick=async()=>{try{await api("/api/logout",{method:"POST"});clearSession();$("#settings").close();notify("已退出账号");}catch(e){notify(e.message);}};
function node(tag,cls,text){const el=document.createElement(tag);if(cls)el.className=cls;if(text!==undefined)el.textContent=text;return el;}
function date(ts){return new Date(ts*1000).toLocaleString("zh-CN",{month:"short",day:"numeric",hour:"2-digit",minute:"2-digit"});}
function select(id){selected=clips.find(c=>c.id===id)||null;document.querySelectorAll(".clip-row").forEach(el=>{el.classList.toggle("selected",el.dataset.id===id);el.setAttribute("aria-pressed",String(el.dataset.id===id));});renderDetail();if(matchMedia("(max-width:900px)").matches&&selected){$("#detail-title").focus();$("#inspector").scrollIntoView({block:"start"});}}
function renderDetail(){const c=selected;$("#detail").hidden=!c;$("#detail-empty").hidden=!!c;$("#inspector").classList.toggle("open",!!c);if(!c)return;$("#detail-kind").textContent=labels[c.kind]||c.kind;$("#detail-title").textContent=c.description;$("#detail-content").textContent=c.text||"此条目包含二进制格式，请在原生客户端预览和复制。";$("#pin").textContent=c.pinned?"取消收藏":"收藏";$("#copy").disabled=!c.text;$("#detail-meta").replaceChildren();for(const [k,v]of [["来源",c.device_id],["时间",date(c.updated_at)],["格式",c.formats.map(f=>f.mime||f.format).join(" · ")],["收藏板",c.collection||"未分类"]]){$("#detail-meta").append(node("dt","",k),node("dd","",v));}}
function render(){const wrap=$("#clips");wrap.replaceChildren();for(const c of clips){const item=node("button","clip-row"+(selected?.id===c.id?" selected":""));item.dataset.id=c.id;item.setAttribute("aria-pressed",String(selected?.id===c.id));item.setAttribute("aria-label",c.description||"剪贴板记录");const body=node("div","clip-body"),title=node("div","clip-title",c.description||"未命名内容");if(c.pinned)title.append(node("span","pin-marker","已收藏"));body.append(title,node("div","clip-snippet",c.text||`${c.formats.length} 个内容格式`));item.append(node("span","clip-kind",glyphs[c.kind]||"T"),body,node("div","clip-time",date(c.updated_at)));item.onclick=()=>select(c.id);const row=node("div");row.setAttribute("role","listitem");row.append(item);wrap.append(row);}$("#result-count").textContent=`${clips.length} 条记录${more?" · 还有更多":""}`;$("#empty").hidden=clips.length>0;$("#more").hidden=!more;if(!token){$("#empty h2").textContent="先连接你的空间";$("#empty p").textContent="登录账号，查看已上传的记录。未上传的本地记录只在对应客户端中可见。";$("#empty-connect").hidden=false;$("#result-count").textContent="连接后查看记录";}if(!clips.length&&token){$("#empty h2").textContent=$("#search").value?"没有匹配的记录":"空间还很安静";$("#empty p").textContent=$("#search").value?"换个关键词，或选择其他内容类型。":"在客户端启用上传后，记录会出现在这里。本地和中继模式不会上传存储。";$("#empty-connect").hidden=true;}renderDetail();}
async function refresh(append=false){if(!token){openSettings();return;}const gen=++generation;currentAbort?.abort();currentAbort=new AbortController();const signal=currentAbort.signal;$("#refresh").disabled=true;try{if(view==="accounts"){await loadAccounts(signal,gen);if(gen!==generation||!currentUser)return;}else if(view==="devices"){const peers=await api("/api/devices",{signal});if(gen!==generation)return;$("#devices").replaceChildren();for(const peer of peers){const card=node("article","device-card");card.append(node("span","tag","在线 · 等待客户端配对"),node("h3","",peer.name),node("code","",peer.code),node("p","","设备身份已登记，只有双方确认绑定后才交换数据。"));$("#devices").append(card);}if(!peers.length)$("#devices").append(node("p","muted","还没有在线设备。请在客户端设置服务器连接。"));}else{const data=await api(`/api/clips?q=${encodeURIComponent($("#search").value)}&filter=${encodeURIComponent(view)}&offset=${append?clips.length:0}`,{signal});if(gen!==generation)return;more=data.length===100;clips=append?[...clips,...data]:data;if(selected)selected=clips.find(c=>c.id===selected.id)||null;render();const stats=await api("/api/stats",{signal});if(gen!==generation)return;$("#nav-count").textContent=stats.clips;$("#pin-count").textContent=stats.pinned;}if(gen!==generation||!currentUser)return;$("#connection-status").textContent=`${currentUser?.username||""} · 已登录`;$("#connection-dot").classList.add("online");$("#connect-top").textContent="连接设置";}catch(err){if(gen===generation&&err.name!=="AbortError"){notify(err.message);$("#connection-status").textContent="连接失败";$("#connection-dot").classList.remove("online");}}finally{if(gen===generation)$("#refresh").disabled=false;}}
$("#refresh").onclick=()=>refresh();$("#more").onclick=()=>refresh(true);$("#search").oninput=()=>refresh();
document.querySelectorAll("[data-view]").forEach(b=>b.onclick=()=>{view=b.dataset.view;selected=null;document.querySelectorAll("[data-view]").forEach(n=>{n.classList.toggle("selected",n===b);if(n===b)n.setAttribute("aria-current","page");else n.removeAttribute("aria-current");});$("#breadcrumb").textContent=labels[view];$("#page-title").textContent=view==="all"?"随时找回，接着使用。":labels[view];$("#accounts-view").hidden=true;$("#library-view").hidden=view==="devices";$("#devices-view").hidden=view!=="devices";refresh();});
function closeDetail() {
  const id=selected?.id; selected=null; renderDetail();
  document.querySelectorAll(".clip-row").forEach(row=>{row.classList.remove("selected");row.setAttribute("aria-pressed","false");});
  document.querySelector(`.clip-row[data-id="${CSS.escape(id||"")}"]`)?.focus();
}
$("#close-detail").onclick=closeDetail;
async function copy(){if(!selected?.text)return;try{await navigator.clipboard.writeText(selected.text);notify("已复制，可以接着使用了");}catch{notify("浏览器未允许复制，请使用 HTTPS 或手动选择文字复制。");}}
$("#copy").onclick=copy;
async function mutation(path,method){if(!selected||busy)return;busy=true;try{await api(`/api/clips/${encodeURIComponent(selected.id)}${path}`,{method});if(method==="DELETE"){selected=null;notify("记录已删除，删除状态会同步到设备。");}await refresh();}catch(err){notify(err.message);}finally{busy=false;}}
$("#pin").onclick=()=>mutation("/pin","POST");$("#delete").onclick=()=>{if(confirm("删除此记录？删除状态会同步到已配对设备和启用存储的客户端。"))mutation("","DELETE");};
$("#export").onclick=()=>{const url=URL.createObjectURL(new Blob([JSON.stringify(clips,null,2)],{type:"application/json"}));const a=node("a");a.href=url;a.download="yank-page.json";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);notify(`已导出当前加载的 ${clips.length} 条记录`);};
document.addEventListener("keydown",e=>{if($("#settings").open||$("#password-dialog").open)return;if(e.key==="Escape"){closeDetail();return;}if(e.target.closest("input,textarea,select"))return;const interactive=e.target.closest("button,a,[role=button]");if(interactive&&!interactive.classList.contains("clip-row"))return;if(e.key==="/"){e.preventDefault();$("#search").focus();}if(e.key==="Enter"&&selected&&interactive?.classList.contains("clip-row")){e.preventDefault();select(interactive.dataset.id);copy();}if(["ArrowDown","ArrowUp"].includes(e.key)&&clips.length){e.preventDefault();let i=clips.findIndex(c=>c.id===selected?.id);i=Math.max(0,Math.min(clips.length-1,i+(e.key==="ArrowDown"?1:-1)));select(clips[i].id);document.querySelector(`.clip-row[data-id="${CSS.escape(clips[i].id)}"]`)?.focus();}});
$("#accounts-nav").onclick=()=>{view="accounts";$("#library-view").hidden=true;$("#devices-view").hidden=true;$("#accounts-view").hidden=false;$("#breadcrumb").textContent="账号管理";$("#page-title").textContent="每个人，都有自己的空间。";refresh();};
async function loadAccounts(signal,gen=generation){const owner=currentUser?.id;const users=await api("/api/accounts",{signal});if(gen!==generation||!owner||currentUser?.id!==owner)return;$("#accounts").replaceChildren();for(const user of users){const card=node("article","device-card");card.append(node("span","tag",user.admin?"管理员":"用户"),node("h3","",user.username));if(user.id!==currentUser?.id){const button=node("button","button danger","删除账号");button.onclick=async()=>{if(!confirm(`删除 ${user.username} 的账号并撤销登录？`))return;try{await api(`/api/accounts/${encodeURIComponent(user.id)}`,{method:"DELETE"});await loadAccounts();}catch(e){notify(e.message);}};const reset=node("button","button secondary","重置密码…");reset.onclick=()=>openPassword(user);card.append(reset,button);}$("#accounts").append(card);}}
$("#create-account").onsubmit=async e=>{e.preventDefault();const button=e.submitter;button.disabled=true;$("#account-error").textContent="";try{await api("/api/accounts",{method:"POST",body:JSON.stringify({username:$("#new-username").value,password:$("#new-password").value,admin:$("#new-admin").checked})});$("#create-account").reset();await loadAccounts();notify("账号已创建");}catch(err){$("#account-error").textContent=err.message;}finally{button.disabled=false;}};
api("/api/session").then(user=>{currentUser=user;token="session";$("#accounts-nav").hidden=!user.admin;refresh();}).catch(()=>{});

let passwordTarget = null;
function openPassword(user) {
  passwordTarget = user.id === currentUser?.id ? null : user;
  $("#settings").close();
  $("#password-form").reset();
  $("#password-error").textContent = "";
  $("#password-title").textContent = passwordTarget ? `重置 ${user.username} 的密码` : "修改密码";
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
  $("#password-error").textContent = "";
  try {
    await api(passwordTarget ? `/api/accounts/${encodeURIComponent(passwordTarget.id)}/password` : "/api/password", {
      method: "POST", body: JSON.stringify({ old_password: $("#old-password").value, new_password: $("#replacement-password").value })
    });
    $("#password-form").reset();
    $("#password-dialog").close();
    if (!passwordTarget) {
      clearSession(); openSettings();
    }
    notify("密码已更新，该账号的全部会话已撤销");
  } catch (err) { $("#password-error").textContent = err.message.includes("8–1024") ? "当前密码不正确，或新密码长度不在 8–1024 字节之间。中文通常每字占 3 字节。" : err.message; }
  finally { button.disabled = false; }
};

for (const id of ["#replacement-password", "#new-password"]) {
  const input = $(id);
  input.oninput = () => {
    const bytes = new TextEncoder().encode(input.value).length;
    input.setCustomValidity(bytes >= 8 && bytes <= 1024 ? "" : "密码需为 8–1024 字节；中文通常每字占 3 字节。");
  };
}
