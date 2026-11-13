"use strict";
const {test} = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const path = require("node:path");
const read = name => fs.readFileSync(path.join(__dirname, name), "utf8");
const source = read("i18n.js"), app = read("app.js"), html = read("index.html");

// Small DOM fixture: textContent removes children, datasets reflect attributes,
// and focus belongs to an element. These semantics catch destructive rerenders.
const attrName = key => "data-" + key.replace(/[A-Z]/g, x => "-" + x.toLowerCase());
class Element {
  constructor(tag, document) {
    this.tagName = tag; this.ownerDocument = document; this.attributes = {};
    this.children = []; this.value = ""; this.checked = false; this.hidden = false;
    this.open = false; this.disabled = false; this._text = "";
    this.dataset = new Proxy({}, {
      get: (_, key) => this.attributes[attrName(key)],
      set: (_, key, value) => {this.attributes[attrName(key)] = String(value);return true;},
      deleteProperty: (_, key) => {delete this.attributes[attrName(key)];return true;}
    });
    this.classList = {
      contains: name => this.className.split(/\s+/).includes(name),
      toggle: (name, active = !this.classList.contains(name)) => {
        const names = new Set(this.className.split(/\s+/).filter(Boolean));
        active ? names.add(name) : names.delete(name); this.className = [...names].join(" ");
      },
      add: name => this.classList.toggle(name, true), remove: name => this.classList.toggle(name, false)
    };
  }
  get className() {return this.attributes.class || "";}
  set className(value) {this.attributes.class = value;}
  setAttribute(name, value) {this.attributes[name] = String(value);}
  getAttribute(name) {return this.attributes[name] ?? null;}
  removeAttribute(name) {delete this.attributes[name];}
  append(...elements) {for (const el of elements) {el.parentElement = this;this.children.push(el);}}
  replaceChildren(...elements) {this._text = "";this.children = [];this.append(...elements);}
  set textContent(value) {this.children = [];this._text = String(value);}
  get textContent() {return this._text + this.children.map(el => el.textContent).join("");}
  matches(selector) {
    const tag = selector.match(/^[\w-]+/), id = selector.match(/#([\w-]+)/), cls = selector.match(/\.([\w-]+)/);
    if (tag && tag[0] !== this.tagName) return false;
    if (id && this.attributes.id !== id[1]) return false;
    if (cls && !this.classList.contains(cls[1])) return false;
    for (const [, name, value] of selector.matchAll(/\[([\w-]+)(?:=["']?([^\]"']+)["']?)?\]/g)) {
      if (!(name in this.attributes) || (value !== undefined && this.attributes[name] !== value)) return false;
    }
    return true;
  }
  querySelectorAll(selector) {
    const parts = selector.split(/\s+/), last = parts.pop(), result = [];
    const walk = el => {for (const child of el.children) {
      if (child.matches(last)) {
        let ancestor = child.parentElement, index = parts.length - 1;
        while (ancestor && index >= 0) {if (ancestor.matches(parts[index])) index--;ancestor = ancestor.parentElement;}
        if (index < 0) result.push(child);
      }
      walk(child);
    }};
    walk(this);return result;
  }
  querySelector(selector) {return this.querySelectorAll(selector)[0] || null;}
  addEventListener(name, listener) {this["on" + name] = listener;}
  focus() {this.ownerDocument.activeElement = this;}
  showModal() {this.open = true;}
  close() {this.open = false;}
  setCustomValidity(message) {this.validationMessage = message;}
  scrollIntoView() {}
  reset() {this.querySelectorAll("input").forEach(input => {input.value = "";input.checked = false;});}
}
function documentFixture() {
  const doc = new Element("document");doc.ownerDocument = doc;
  doc.createElement = tag => new Element(tag, doc);
  const stack = [doc];
  for (const token of html.match(/<[^>]+>|[^<]+/g)) {
    if (token.startsWith("<!")) continue;
    if (token.startsWith("</")) {stack.pop();continue;}
    if (token.startsWith("<")) {
      const tag = token.match(/^<([\w-]+)/)[1], el = doc.createElement(tag);
      for (const [, key, value] of token.matchAll(/\s([\w-]+)(?:="([^"]*)")?/g)) el.setAttribute(key, value || "");
      el.hidden = "hidden" in el.attributes;
      stack.at(-1).append(el);
      if (!token.endsWith("/>") && !["meta","link","input","img","br"].includes(tag)) stack.push(el);
    } else {
      const text = doc.createElement("#text");text._text = token;stack.at(-1).append(text);
    }
  }
  doc.documentElement = doc.querySelector("html");
  return doc;
}
function setup({saved, languages = ["en-US"], document = documentFixture(), fetch} = {}) {
  const values = new Map(saved === undefined ? [] : [["yank.language", saved]]);
  const storage = {getItem: key => values.get(key) ?? null, setItem: (key,value) => values.set(key,value), removeItem: key => values.delete(key)};
  const context = vm.createContext({document, navigator:{languages}, localStorage:storage, sessionStorage:storage,
    TextEncoder, AbortController, URL, Blob, CSS:{escape: value => value},
    matchMedia: () => ({matches:false, addEventListener(){}}),
    setTimeout: () => 1, clearTimeout(){}, confirm: () => false,
    fetch: fetch || (async () => ({ok:false,status:401}))});
  vm.runInContext(source, context);
  return {context, document, values, i18n:context.YankI18n, run: code => vm.runInContext(code, context)};
}
const placeholders = value => [...value.matchAll(/\{(\w+)\}/g)].map(m => m[1]).sort();

test("zh/en dictionaries have matching nonempty keys and interpolation placeholders", () => {
  const {i18n} = setup();
  const {zh,en} = i18n.dictionaries;
  assert.deepEqual(Object.keys(zh).sort(), Object.keys(en).sort());
  for (const key of Object.keys(zh)) {
    assert.ok(zh[key].trim(), `Empty zh: ${key}`);assert.ok(en[key].trim(), `Empty en: ${key}`);
    assert.deepEqual(placeholders(zh[key]), placeholders(en[key]), key);
  }
});

test("all static text and accessible attributes have dictionary bindings", () => {
  const {document,i18n} = setup();
  const neutral = /^(?:yank|Yank|T|↗|▧|▤|—|\/|↻|Enter|Esc)$/;
  function walk(el, bound = false) {
    bound ||= !!el.dataset.i18n;
    if (el.tagName === "#text" && el.textContent.trim() && !bound) assert.match(el.textContent.trim(), neutral, `Unbound text: ${el.textContent}`);
    for (const attribute of ["aria-label","placeholder","title"]) {
      const value = el.getAttribute(attribute);
      if (value && value !== "Yank") assert.ok(el.getAttribute(`data-i18n-${attribute}`), `Unbound ${attribute}: ${value}`);
    }
    for (const [attribute,key] of Object.entries(el.attributes)) if (attribute.startsWith("data-i18n") && attribute !== "data-i18n-params") {
      assert.ok(Object.hasOwn(i18n.dictionaries.en,key), `Unknown static key: ${key}`);
    }
    for (const child of el.children) walk(child,bound);
  }
  walk(document);
  assert.ok(html.indexOf('src="/i18n.js"') < html.indexOf('src="/app.js"'));
  assert.doesNotMatch(read("app.css"), /aria-label="主导航"/);
});

test("dynamic strings use known keys and have no untranslated prose", () => {
  const {i18n} = setup();
  const keys = [...app.matchAll(/(?:msg|t|uiError)\("([\w.]+)"/g)].map(m => m[1]);
  // Status-to-key mapping and conditional keys are also checked, including keys
  // that aren't the first argument of msg().
  keys.push(...[...app.matchAll(/"((?:request|auth|password|accounts|records)\.[\w]+)"/g)].map(m=>m[1]));
  for (const key of keys.filter(key => key !== "theme.")) assert.ok(Object.hasOwn(i18n.dictionaries.en,key), key);
  for (const theme of ["system","light","dark"]) assert.ok(Object.hasOwn(i18n.dictionaries.en,`theme.${theme}`));
  assert.doesNotMatch(app, /\p{Script=Han}/u, "Chinese UI literals belong in the dictionary");
  assert.doesNotMatch(app, /\.textContent\s*=/, "Use setText so dynamic messages retranslate");
  assert.doesNotMatch(app, /\.innerHTML|insertAdjacentHTML/, "Keep translated and user content as text");
  // Audit every quoted literal, including single-word labels. The explicit
  // exceptions are protocol names, CSS selectors/classes, tags, keys and glyphs.
  const structural = new Set([
    "", " selected", " · ", "—", "↗", "▤", "▧", "T", "use strict",
    "(max-width:900px)", "(prefers-color-scheme: dark)", ".clip-row", "[data-view]",
    "button,a,[role=button]", "input,textarea,select", "/", "AbortError", "ArrowDown", "ArrowUp",
    "Content-Type", "DELETE", "Enter", "Escape", "POST", "application/json",
    "aria-current", "aria-label", "aria-pressed", "role", "listitem", "page",
    "a", "article", "button", "code", "dd", "div", "dt", "h3", "p", "span",
    "button danger", "button secondary", "clip-body", "clip-kind", "clip-row", "clip-snippet",
    "clip-time", "clip-title", "device-card", "muted", "pin-marker", "tag",
    "accounts", "all", "devices", "change", "dark", "false", "keydown", "light", "online",
    "file-text", "list-ordered", "monitor", "palette", "folder", "clipboard", "kubai-icon clip-kind", "img", "src", "alt", "aria-hidden", "width", "height", "28", "true", "/icons/", ".svg",
    "link", "image", "code", "color", "files", "copy", "search", "star", "settings", "trash", "rotate-cw", "users", "download", "checkbox", "arrow-right", "shield-check",
    "open", "selected", "session", "start", "system", "theme.", "yank-page.json", "yank.appearance", "yank.token"
  ]);
  const quotedSource = app.replace(/`([^`]*?)`/g, (_, template) => {
    assert.ok(template.startsWith("/api/") || template.startsWith(".clip-row["), `Unkeyed template: ${template}`);
    return "";
  });
  for (const match of quotedSource.matchAll(/"([^"\n]*)"|'([^'\n]*)'/g)) {
    const literal = match[1] ?? match[2];
    assert.ok(Object.hasOwn(i18n.dictionaries.en,literal) || structural.has(literal) || literal.startsWith("#") || literal.startsWith("/api/") || literal === "/pin", `Unkeyed dynamic literal: ${literal}`);
  }
});

test("browser preferences, stored locale, invalid values and denied storage fall back safely", () => {
  const {i18n} = setup();
  const stored = value => ({getItem: () => value});
  assert.equal(i18n.initialLanguage(stored("zh"),["en-US"]),"zh");
  assert.equal(i18n.initialLanguage(stored("en"),["zh-TW"]),"en");
  for (const invalid of ["ZH","fr","zh-CN","__proto__","",null]) assert.equal(i18n.initialLanguage(stored(invalid),["zh-TW"]),"zh");
  assert.equal(i18n.initialLanguage({getItem(){throw Error("denied");}},["en-GB"]),"en");
  assert.equal(i18n.browserLanguage(["fr-FR","zh-HK","en"]),"zh");
  assert.equal(i18n.browserLanguage(["de-DE"]),"en");
  assert.equal(i18n.browserLanguage([]),"en");
  const state = setup({saved:"invalid",languages:["zh-CN"]});
  assert.equal(state.i18n.language,"zh");state.i18n.setLanguage("en");
  assert.equal(state.values.get("yank.language"),"en");
});

test("live switching retains DOM identity, draft values, focus, selection, dialogs and request state", async () => {
  let calls = 0;
  const state = setup({fetch:async () => {calls++;return {ok:false,status:401};}});
  state.run(app);await new Promise(resolve=>setImmediate(resolve));
  const {document,i18n,run} = state;
  run(`token="session";currentUser={id:"me",username:"Alice",admin:true};view="pinned";
    clips=[{id:"clip-1",kind:"files",description:"",text:"",pinned:true,updated_at:1700000000,formats:[{mime:"text/plain"}],device_id:"Laptop",collection:""}];
    selected=clips[0];more=true;render();
    setText($("#breadcrumb"),labels.pinned);
    setText($("#connection-status"),msg("status.signedIn",{username:"Alice"}));
    passwordTarget={id:"other",username:"Bob"};
    setText($("#password-title"),msg("password.resetTitle",{username:"Bob"}));
    setText($("#password-error"),msg("password.error"));
    notify(msg("export.done",{count:1}));
    $("#password-dialog").showModal();`);
  const row = document.querySelector(".clip-row"), fields = ["search","token","new-username","new-password","old-password","replacement-password"];
  for (const id of fields) document.querySelector("#"+id).value = "draft-"+id;
  const focused = document.querySelector("#replacement-password");focused.selectionStart=2;focused.selectionEnd=6;focused.focus();
  const requests = calls;
  i18n.setLanguage("zh");
  assert.equal(document.activeElement,focused);assert.equal(focused.selectionStart,2);assert.equal(focused.selectionEnd,6);
  assert.equal(document.querySelector(".clip-row"),row);assert.equal(row.getAttribute("aria-label"),"剪贴板记录");
  assert.equal(document.querySelector("#password-dialog").open,true);
  assert.equal(document.querySelector("#password-title").textContent,"重置 Bob 的密码");
  assert.equal(document.querySelector("#result-count").textContent,"1 条记录 · 还有更多");
  assert.match(document.querySelector("#connection-status").textContent,/Alice.*已登录/);
  assert.match(document.querySelector("#toast").textContent,/已导出/);
  assert.equal(document.documentElement.lang,"zh-CN");
  i18n.setLanguage("en");
  assert.equal(document.querySelector("#search").getAttribute("placeholder"),"Search content, titles or collections…");
  assert.equal(document.querySelector("#password-title").textContent,"Reset password for Bob");
  assert.match(document.querySelector("#password-error").textContent,/current password is incorrect/);
  assert.equal(document.querySelector("#breadcrumb").textContent,"Pinned");
  assert.equal(document.querySelector("#detail-content").textContent,i18n.t("binary.hint"));
  assert.equal(document.querySelector("#theme").textContent,"Appearance · System");
  assert.equal(document.activeElement,focused);assert.equal(document.querySelector(".clip-row"),row);
  assert.equal(document.documentElement.lang,"en");
  for (const id of fields) assert.equal(document.querySelector("#"+id).value,"draft-"+id);
  document.querySelectorAll("[data-language]").forEach(el => assert.equal(el.value,"en"));
  assert.equal(run("view"),"pinned");assert.equal(run("selected.id"),"clip-1");assert.equal(run("more"),true);
  assert.equal(calls,requests,"Language changes must not fetch or reset request state");
});

test("interpolation remains text, dates follow locale and ordinary content is untouched", () => {
  const {i18n,document} = setup();
  const el = document.createElement("p");document.append(el);
  i18n.setText(el,i18n.msg("status.signedIn",{username:'<img src=x onerror="alert(1)">'}));
  assert.match(el.textContent,/<img/);assert.equal(el.children.length,0);
  const ordinary = document.createElement("pre");document.append(ordinary);i18n.setText(ordinary,"User data 中文 English {count}");
  const enDate = i18n.t("date",{timestamp:1700000000});
  i18n.setLanguage("zh");assert.notEqual(i18n.t("date",{timestamp:1700000000}),enDate);
  assert.equal(ordinary.textContent,"User data 中文 English {count}");
  assert.match(el.textContent,/已登录/);assert.equal(el.children.length,0);
  assert.throws(()=>i18n.t("missing.key"),/Unknown translation key/);
});

test("API failures and UTF-8 password validity remain translated during switching", async () => {
  let response = {ok:false,status:400};
  const state = setup({fetch:async () => response});state.run(app);
  await new Promise(resolve=>setImmediate(resolve));
  for (const [status,key] of [[401,"auth.error"],[403,"request.forbidden"],[404,"request.missing"],[409,"request.conflict"],[429,"request.rate"],[503,"request.server"]]) {
    response = {ok:false,status};
    await assert.rejects(state.run('api("/api/clips")'),error=>error.localized.key===key);
  }
  response={ok:false,status:400};
  await assert.rejects(state.run('api("/api/accounts/u/password")'),error=>error.localized.key==="password.error");
  await assert.rejects(state.run('api("/api/accounts")'),error=>error.localized.key==="accounts.invalid");
  const input=state.document.querySelector("#replacement-password");input.value="中文";input.oninput();
  assert.match(input.validationMessage,/8–1024 bytes/);
  state.i18n.setLanguage("zh");assert.match(input.validationMessage,/8–1024 字节/);
  input.value="中文abc";input.oninput();assert.equal(input.validationMessage,"");
});

test("untitled pinned record keeps its marker and translates both sibling labels", () => {
  const {run, document, i18n} = setup();
  run(app);
  run(`token="session";clips=[{id:"untitled",kind:"text",description:"",text:"clipboard text",pinned:true,updated_at:1700000000,formats:[]}];render();`);
  const title = document.querySelector(".clip-title");
  const marker = title.querySelector(".pin-marker");
  assert.ok(marker);
  for (const [locale, label, pinned] of [["zh","未命名内容","已收藏"],["en","Untitled content","Pinned"],["zh","未命名内容","已收藏"]]) {
    i18n.setLanguage(locale);
    assert.equal(title.querySelector(".pin-marker"), marker, "Switching must not remove or recreate the pin marker");
    assert.equal(marker.textContent, pinned);
    assert.equal(title.children[0].textContent, label);
    assert.notEqual(title.children[0], marker, "Translated title and marker must be siblings");
  }
});

test("code and color details translate their kind without changing clipboard text", () => {
  const {run, document, i18n} = setup();
  run(app);
  for (const [kind, zh, en, content] of [["code","代码","Code","const message = '中文 English';"],["color","颜色","Color","#a1b2c3"]]) {
    run(`clips=[{id:"kind-record",kind:${JSON.stringify(kind)},description:"User title 中文",text:${JSON.stringify(content)},pinned:false,updated_at:1700000000,formats:[],device_id:"Laptop"}];selected=clips[0];renderDetail();`);
    const detail = document.querySelector("#detail-content");
    for (const [locale, label] of [["zh",zh],["en",en],["zh",zh]]) {
      i18n.setLanguage(locale);
      assert.equal(document.querySelector("#detail-kind").textContent,label);
      assert.equal(document.querySelector("#detail-content"),detail);
      assert.equal(detail.textContent,content);
      assert.equal(document.querySelector("#detail-title").textContent,"User title 中文");
      assert.equal(run("selected.text"),content);
    }
  }
});
