# Yank 0.2 UI/UX 最终 Web 复测与原生待验记录

审核冻结结论（2026-11-13 UTC）：已限定静态复核 `pref_text` 局部草稿/每字段显式保存、设备码 Snapshot 订阅及外观持久化三态修复，**未发现这三处仍存的 P0/P1，设备码订阅 P2 已静态关闭**。设置数字项“保留天数”的前轮 P2 保留，不影响上述限定结论，亦不宣称所有设置均已迁移。最新证据见文末“外观持久化限定补核与最终冻结”。

最新有界平台实测（2026-11-13 UTC）：已通过 Android 当前 APK 正常触摸保存既有草稿，并只读确认 Linux 同 ID 正文收到 `Android_reply_verified`，反向编辑同步成立。详见末尾 Android → Linux 反向同步实测；不代表新版打包或完整平台验收。

历史静态补注（设备码 P2 已由文末冻结结论覆盖；2026-11-13 UTC）：`7973ed4` 的网络 Snapshot 失效修复已核读；页脚/发现列表更新链路成立，未发现该链路新增 P0/P1；本机设备码缺少响应式订阅仍有一项 P2，见末尾定向复核。未执行新版 APK/原生跨设备验收。

前轮状态（仅针对当次要求的 3 项 Web P2）：**P2-01、P2-02、P2-03 均已定向复测关闭**，详见末尾“Web 三项 P2 最终定向复测”。其余平台边界和原生待验项保持原范围；下文保留前轮发现作为审核历史。

初次 Web 复测记录（2026-11-13 UTC）：**当时原 Web P0/P1 均已关闭，没有发现新的已确认代码 P1；当时仍有 3 项 P2。该结论仅覆盖本轮 Web 检查，不代表整版、原生桌面、Android 或自有同步已验收。** 原生 UI 正由 Herschel 处理，完成后再补独立复审。

本版范围为 **Yank 自有同步**；**微信输入法不在本版**。本审核没有修改业务代码、构建、重启或发布；只写本报告。已获授权创建临时本地账号测试本人改密和管理员重置，未改动 review 主账号口令。临时账号已删除，辅助会话已失效，浏览器最终退出并关闭；本报告不包含任何测试口令或会话凭据。

## 本轮证据与边界

- 沿用 ui-ux-pro-max、apple-design、animate、nsh-design-guides 及其 design-guides.md，使用 agent-browser 独立会话 `yank-ux-audit`。
- 针对本轮延续工作执行 Nowledge CLI 检索 `nmem --json t search 'Yank UI UX password followup Herschel'`，返回 0 条线程。
- 用户通知重建重启后，访问 `http://127.0.0.1:17219`，逐字节核对 HTML/CSS/JS 与工作树一致。浏览器为本地 Chromium；检查 1440×1000、390×844、320×720，以及浅色、深色下的部分可达状态。
- 测试账号原本无记录。复制和详情回归使用仅在页面内存的 A/B 样本，不写服务器；替换 clipboard.writeText 只采集复制参数。因此证明的是“传递给复制 API 的内容正确”，未证明系统剪贴板最终写入。reload 后已清除样本与替身。
- 账号创建、本人改密、管理员重置、错误口令和会话失效使用真实本地接口；主要表单通过正常 UI 操作。重置后的旧/新口令检查及辅助会话失效检查使用独立 HTTP cookie jar，cookie 仅在进程内存，未持久化。临时账号通过已认证 API 删除，未将其清理称作删除按钮的交互验收。
- 账号列表退出竞态使用受控诊断：只在浏览器延迟交付 `/api/accounts` 的真实响应，退出完成后再放行；没有伪造服务器响应或修改服务器数据。
- 本轮未运行原生程序、Android、系统屏幕阅读器、软键盘、真实设备配对/同步/上传、Rust 测试或构建。浏览器手机宽度不能代替 Android 平台验证；axe 零已确认违规不能代替完整无障碍合规验收。

## 原失败路径复测结果

| 问题 | 最新源码位置 | 复测结果 |
| --- | --- | --- |
| UX-01 缺失退出控件导致初始化中断 | `/home/czyt/code/rust/yank/crates/yank-server/web/index.html:43` | 正常登录、账号导航、改密和退出入口均可操作；原 P0 关闭。 |
| F-01 / UX-05 焦点 B 却复制 A | `/home/czyt/code/rust/yank/crates/yank-server/web/app.js:51` | 点 A → Tab 到 B → Enter：焦点和详情均为 B，采集到的复制参数为 B。当前 Enter 分支先 select 当前焦点 ID，再 copy；原 P1 关闭。 |
| F-02 / UX-06 Escape 丢焦点 | `/home/czyt/code/rust/yank/crates/yank-server/web/app.js:40`、`:51` | 桌面选 B 后 Escape：详情隐藏，焦点仍在 B，所有行 aria-pressed=false 且无 selected 类；390px 选 A 后 Escape 同样回 A。统一 closeDetail 不再替换列表。原 P1 关闭。 |
| F-03 普通退出后残留已登录状态 | `/home/czyt/code/rust/yank/crates/yank-server/web/app.js:19`、`:31`、`:36` | 正常退出后状态为“尚未登录”、online=false、计数为“—”、账号/设备缓存为空、空态为“先连接你的空间”、连接按钮可见，session 返回 401。普通路径已修复；慢账号响应分支仍见 P2-01。 |
| UX-08 原浅色侧栏对比度 | `/home/czyt/code/rust/yank/crates/yank-server/web/app.css:1` | 新 muted 配色保留；本轮桌面可达空列表 axe 无已确认违规。未重新出现初审 4.35:1 缺陷。 |
| UX-11 窄屏导航文字挤断 | `/home/czyt/code/rust/yank/crates/yank-server/web/app.css:9` | 新导航覆盖仍在；本轮 320px 无整页横向溢出。原修复保持。 |

## 新密码生命周期的真实验证

| 场景 | 结果 |
| --- | --- |
| 创建临时普通账号 | 管理员正常创建，未勾选管理员权限，账号可登录。 |
| 打开本人改密 | 有当前/新密码标签；当前密码 required，新密码有 required 和 minlength；取消/初始焦点可达。 |
| 新密码过短 | 点击提交后浏览器 validity.tooShort=true，焦点落新密码，表单保留，按钮未锁死。 |
| 当前密码错误 | 真实接口拒绝，错误出现在表单 alert 区，弹窗保持打开，按钮恢复。中文体验问题见 P2-03。 |
| 本人改密成功 | 表单清空并关闭，重新打开登录框，显示会话撤销说明；当前 session 返回 401，页面状态为尚未登录。 |
| 使用改密前后口令 | 旧口令通过正常登录表单被拒绝，新口令登录成功。 |
| 管理员重置 | 标题明确指定临时目标账号；当前密码字段隐藏且不再 required，正常提交成功。未重置主账号。 |
| 重置使另一个活跃会话失效 | 重置前专门建立的临时账号辅助会话，在重置后查询 session 返回 401。说明被测旧会话确实失效；不扩展为所有平台会话测试已完成。 |
| 重置前后口令 | 独立 HTTP 登录检查：重置前口令 401，重置后口令 200；检测成功后的辅助会话已退出。 |
| 管理员自身会话 | 重置他人后仍为 200，没有被误清。 |
| 密码框 Escape | 关闭密码框，回到 connect-top 可见按钮，两个 dialog 都已关闭。管理员重置成功后焦点回“重置密码…”触发器。 |
| 测试清理 | 临时账号删除返回 204，随后账号列表确认已不存在，review 主账号仍存在且原口令可登录；最终测试浏览器退出。 |

这些是实际经过的路径，没有执行 review 主账号的改密或重置。

## 仍需修改的 P2

### P2-01：退出时账号列表慢响应仍可回写“已登录”与缓存（F-03 的残余分支）

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:37`（refresh 的 accounts 分支及尾部登录展示）、`:53`（loadAccounts）、`:19`（clearSession）。
- 原因：clearSession 已 abort currentAbort 并递增 generation，但 loadAccounts 没有接收 signal，也没有在 await 后检查 generation/账号身份；refresh 在 accounts 分支返回后仍无条件写“已登录”。
- 受控诊断复现：管理员账号页刷新，延迟交付真实 accounts 响应 → 正常退出 → 状态先为尚未登录、账号缓存为 0 → 放行旧响应。此时 session 仍为 401，界面却变成“· 已登录”、online=true，隐藏的账号列表又出现 2 个卡片。
- 影响：集中清理在慢请求场景下被旧任务覆盖。当前证据是 UI/内存缓存回写；没有发现退出后接口仍授权或记录泄露的证据，不将其描述为会话撤销失败。
- 最小修复：loadAccounts 接入本次请求的 signal/generation，在 DOM 写入前检查；refresh 的成功/失败状态展示也统一检查 generation 与当前会话。直接触发的 loadAccounts 调用同样绑定会话身份，防止换账号后旧响应提交。
- 回归：分别延迟账号、记录、设备响应，在退出或换账号后放行，旧请求都不得写 DOM、缓存、登录状态或错误提示。

### P2-02：新改密输入框未复用现有字段样式，实测高度仅 27px

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/index.html:44`；`/home/czyt/code/rust/yank/crates/yank-server/web/app.css:3`（只有旧 token 字段的特定样式）、`:7`（username/account-form 字段）。
- 证据：正常 UI 打开改密后，两个密码 input 均实测 189×27px。新 old-password/replacement-password 没有覆盖旧表单的 block、全宽、padding、min-height、border 和主题表面规则。桌面截图明确显示字段明显短于表单；320px 深色下仍为 27px，使用浏览器默认输入框表面。
- 影响：手机直接触摸目标偏小，字段与既有登录表单不一致。没有把此问题误写成 axe 已报出的 contrast violation。
- 最小修复：提取/复用统一表单 input 类，覆盖新旧密码字段，手机至少 44px 高、合理全宽与间距，并从现有语义 tokens 取色；保留清楚焦点样式。
- 回归：320/390px 及桌面、浅/深色下测量控件尺寸并查看真实排版；本轮未验证软键盘。

### P2-03：改密失败提示仍直接显示英文服务端错误，长度单位也未说清

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:14`（除 401 外直接使用 e.error）、`:86`（密码表单呈现）；`/home/czyt/code/rust/yank/crates/yank-server/web/index.html:44`；对应服务端错误 `/home/czyt/code/rust/yank/crates/yank-server/src/main.rs:413`。
- 实测错误为 `Incorrect current password or invalid new password (8–1024 bytes required)`。这条错误是真的失败反馈，但在中文表单中难以直接理解，还将当前密码错误与新密码长度合在一句。HTML minlength/maxlength 的字符长度规则与后端按 UTF-8 bytes 计数也不是同一单位。
- 最小修复：以稳定错误码映射简明中文，当前密码、新密码分别定位；帮助文本与校验采用一致且明确的长度规则。保留未知错误的可信兜底，不以统一“已更新”覆盖真实失败。不必为修文案引入复杂表单框架。
- 回归：错误当前密码、短新密码、过长及非 ASCII 新密码、会话过期，均显示实际错误且可重试。

## 对比度、布局与动效结果

- 桌面可达页面 axe 4.12.1：0 个 confirmed violations；空 role=list 和部分字形仍为 incomplete，未计作已确认错误。
- 320px 浅色改密弹窗：axe 0 violations、0 incomplete。320px 深色改密弹窗：0 violations、1 个说明文字 background/overlap 的 incomplete；确认弹窗 opacity=1 后仍有 incomplete，需人工判断，不凭此宣称失败或完整通过。
- 320px 已登录设置弹窗：页面 scrollWidth=320，dialog 未出现横向溢出；新增按钮可达。截图与测量同时暴露了 P2-02，不把“无横向滚动”当所有表单体验通过。
- 未添加复制/搜索/列表选择等高频操作动画。没有要求以 Apple 风格为由增加动效；reduced-motion 的原静态覆盖仍在。本轮未测试系统层面的减弱动态偏好切换。

## 原生 Herschel 进展与下一轮范围

本轮先前静态看到：按记录 ID 保存 draft map、dirty 提示、“复制草稿文字”、pending()/feedback() 驱动局部文案与按钮、文件对话框失败反馈，以及读取 prefs.mode 的 radio_group。这些方向覆盖先前 P2 的主要缺口，但当前原生 UI 仍在开发，**不据此关闭原生条目或判断平台通过**。

待 Herschel 通知完成后，应核对最新行号与最终实现，再验证：草稿切换/返回/保存后状态、异步快照与草稿对应关系、模式组可访问名称/键盘、任务成功/失败反馈、窄桌面 849/850px、Android 系统返回与软键盘，以及上传和账号选择的真实同步边界。微信输入法不加入本版验收。

## 结论

本轮 Web 原代码 P1 已完成针对性复测关闭；未发现新增已确认 P0/P1。上述 3 项 P2 均有具体证据与最小修复，建议在发布前处理，尤其是退出后的旧请求回写。**这不是整版发布批准**：原生独立复审与平台运行仍未完成。下一轮只需聚焦 P2 修复及完成后的原生实现，不必重新验证无关范围。

## 复测版本快照

最终核对时间：2026-11-13T05:22:09+00:00。

| 文件 | SHA-256 前 16 位 | 与运行服务一致 |
| --- | --- | --- |
| `crates/yank-server/web/index.html` | `70c88e52fc7b70a2` | 是 |
| `crates/yank-server/web/app.js` | `696622976e0719f0` | 是 |
| `crates/yank-server/web/app.css` | `0fc660e145fc8ae8` | 是 |

## Android 首页截图与 UI dump 独立补审（只读）

用户随后提供实际启动证据：Android 35 x86_64 模拟器、支持 arm64 翻译，包名 `io.github.cax.yank`；截图 `/tmp/yank-android-native.png`，UI dump `/tmp/yank-android-ui.xml`。本审核独立打开截图并解析 dump，**没有运行 ADB、没有点击/导航，也没有干扰正在安排的键盘测试**。Nowledge 定向检索 `Yank Android UI emulator audit` 返回 0 条线程。

此项把原先“完全没有原生运行证据”更新为：**用户已实际启动该 Android 模拟器，所给证据中的首页已可见渲染；本审核只验证了静态截图/dump**。截图不能证明输入、剪贴板写入、系统返回、账号、同步、上传或配对通过；没有正式验证真实 arm64 硬件。Linux 当前 X100 窗口黑屏由用户报告，原因仍待排查，**不计为通过**，也不据此断定一定是应用 UI 或一定是 renderer 的责任。

### 截图可确认的情况

- 标题、账号可选提示、手动记录入口、导航、搜索、空态和离线状态均可见；首页没有整页黑屏。页面没有出现截图可见的横向裁切，内容避开了底部系统手势区。
- “加载更多记录”在截图里呈灰色，dump 对应 Button 为 enabled=false、clickable=false、focusable=false，与空结果下不可用的状态一致。
- 截图/树反映的是当前首页空态，不能推为其他屏幕、深色、大字、软键盘或横屏通过。dump 的坐标单位是截图像素，本次未读取设备密度，**不把像素高度换算成 dp 来宣布触摸目标达标**。
- 下述源代码位置用于定位当前实现；没有比对 APK 构建哈希，不承诺该运行 APK 与此刻并行修改的源代码逐字一致。

### 建议修改的 P2

1. **导航标签拆行且没有当前页标记。** 截图中“顺序复制”显示为“顺序复 / 制”，其他四项为单行；五个按钮外观相同。dump 五项均 selected=false，当前源码 `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:722` 的 mobile_nav 也全部使用相同 action，没有读取 page 决定选中状态。建议将导航短标签改为“队列”等能完整呈现的词，页面标题保留“顺序复制”；为当前目的地提供选中视觉和可访问性语义。不用缩小中文字号来塞入长标签。
2. **触屏首页仍突出显示桌面键盘提示。** 截图与 dump 均有“按 ↑ ↓ 选择 · Ctrl+Enter 复制”；当前位置 `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:851`。建议在触屏默认路径显示结果数与适合当前操作的提示，桌面或外接键盘场景再显示快捷键。这是信息层级建议，不代表 Android 不能使用外接键盘。
3. **高级筛选操作先于核心列表，挤占首屏。** 截图中搜索已有可见标签，placeholder 又重复整句；紧接着是占满行的“保存筛选为智能列表”，列表区域从 dump 的 y=1052 才开始，约为 2400px 屏幕高度的 44%。位置 `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:835`、`:844`。建议保留可见标签，placeholder 简化为输入示例；将保存筛选降为轻量次要动作或放到筛选菜单。空结果时优先突出手动记录与内容区域，不必所有能力都占一个满宽主控件。
4. **Android“暂停记录”的作用范围需要说清。** 截图同时显示“记录剪贴板”和“暂停记录”；当前实现 `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:774`、`:779` 将该项绑定 capture 偏好。鉴于本版 Android 采用前台手动记录，建议明确暂停的究竟是自动记录、接收还是某种采集；若 Android 上没有对应自动记录任务，应隐藏或调整该入口，避免用户以为有持续后台采集。这项需要结合最终 Android worker 行为核实；本审核没有点击暂停或证明其实际效果。

### 优先核查的可访问性风险（未定为已确认 P1）

dump 中“记录剪贴板”“暂停记录”和导航等 Button 的 text/content-desc 均为空，可见文案出现在其不可单独聚焦的 ViewGroup 子节点。`/home/czyt/code/rust/yank/crates/yank-client/src/ui/controls.rs:4` 的通用 action 目前把 label 放在 child text，未在调用点显式设置按钮可访问名称。

**仅凭 UI dump 不能断言 TalkBack 一定会读成空按钮**：平台可能合并子文本。本轮不启用 TalkBack或抢占输入。建议下一轮用 TalkBack/可访问性节点检查确认每个按钮读出的名称和操作，必要时在通用 action 层显式提供名称，并保留子文字显示。若最终确实出现未命名的可聚焦按钮，再升级为已确认 P1。

### 平台状态更新

- Android 35 x86_64 模拟器（含 arm64 翻译）：有用户实际启动和本审核截图/dump 检查证据；交互、键盘和同步另验。
- 真实 arm64 Android 硬件：未正式验证。
- Linux X100：窗口黑屏，未通过，原因未定。
- 本轮新增结果是静态可视建议和待验风险，不撤销 Web 已关闭的原 P1，也不把原生或整版状态改为发布通过。

## 原生最终静态复审及 Linux 渲染证据补充

本节在 Herschel 完成原生 UI、用户通知源码稳定后追加，**替代前文“原生仍在开发、等待静态复审”的状态**。结论：**当前原生 UI 没有发现仍成立的已确认代码 P0/P1；原生代码层面不再因初审 UX-02/03/04/07 阻塞。平台验收仍未完成，不等同于 Android 新 APK、真实 arm64 硬件或完整 Linux 窗口通过。**

### 本轮检查与测试证据边界

只读检查 `ui/mod.rs`、`controls.rs`、`desktop.rs`、theme、全部 12 项 interface 测试源码、2 项 UI unit 测试源码，以及与保存/登录/退出关联的 worker 分支。用户报告 12 interface tests、2 UI unit 和 clippy 已通过；**本审核没有重新运行这些命令，也没有将其写成本审核亲自观察到的测试输出**。源码中的测试已包含真实交互、状态和持久化断言，覆盖强度明显高于初审的两个文案测试。

Nowledge 本轮检索 `Yank native UI Herschel interface final audit` 返回 0 条线程。没有执行 ADB、触摸、原生键盘操作或启动/终止用户应用，没有打断新 APK 验证；没有修改业务代码。

### 原问题最终静态判断

| 原问题 | 当前判断 | 可核对证据 |
| --- | --- | --- |
| UX-02 同服务器“不关联”仍上传 | **原代码 P1 静态关闭** | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1420` 无条件 detach；`:38`、`:153`、`:158` 在队列保存前收回共享权限，只减权、不提前开启上传。UI unit `:1851` 覆盖排队设置不能重新授予上传、解除关联清账号。真实网络中的在途请求边界仍另验。 |
| UX-03 窄桌面详情不可达 | **原代码 P1 静态关闭** | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:124` 的 choose 与 `:519` 侧栏条件一致；`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:102` 明确设置 849/850px，验证独立详情、返回和双栏行为。 |
| UX-04 保存覆盖二进制内容 | **原代码 P1 静态关闭** | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:29` 要求所有格式为文字才能编辑；`:1026` 只读、`:1033` 说明格式后果、`:1066` 非文字保持原 text，仅更新收藏板。`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:258`，验证收藏板更新后 text 和 formats 原样保留；UI unit `mod.rs:1877` 覆盖“有文字 fallback 的 native binary”不可编辑。 |
| UX-07 删除事前范围 | **原代码 P1 静态关闭** | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1078` 使用 destructive 确认，说明已配对设备及服务器的传播范围。未执行原生实际删除与传播。 |
| UX-09 pending 与错误反馈 | **主要 P2 静态修复；宣告语义另验** | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:190`、`:193`、`:1418`、`:1419`：pending 读取、登录防重复提交、就近反馈；`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:341` 用延迟失败响应验证登录/导入/Ditto 禁用，以及表单和页底均出现真实错误。文件选择失败已不再静默吞掉。 |
| UX-10 模式看不出选中项 | **原 P2 静态关闭** | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1370` radio_group，`:1380` 绑定当前值，`:1392` checked 样式；`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:165` 断言 radio checked 互斥、保存值、390px 内布局和至少 48pt 高度。该测试是 headless 几何，不是 Android 原生触摸实测。 |
| UX-12 草稿丢失/复制对象不清 | **主要 P2 静态关闭** | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:166` 按 ID 保存草稿；`:364` 只清理已与保存结果相等的草稿；`:1042` 有未保存提示，`:1043`、`:1044` 区分已保存内容与草稿。`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:125` 验证 A/B 草稿隔离、两种复制对象、模拟软键盘下返回、离开后保留与保存后状态。 |
| UX-13 测试只有文案 | **覆盖缺口显著收敛，平台边界保留** | 上述行为测试加上安全预览、队列、失败恢复、浮窗关闭后重开等。Fixture 明确设置 Linux 或 iOS 环境及 viewport；phone fixture 使用 `Os::Ios`，所以这些测试不能称作 Android 测试。Android 新 APK 仍需实际运行确认。 |

### 与现有 Android 截图的对应关系

原截图证据仍有效，但不是最终 APK 的视觉验收：

- 当前源码导航已由“顺序复制”改为“队列”（`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:728`），旧截图的长标签断行不再是现源码文字；待新 APK 确认实际显示。
- 非桌面标题提示已改为“前台手动记录”，暂停/继续仅桌面显示（同文件 `:759`、`:783`）。旧截图上的“暂停记录”不用于认定新 APK 仍有同一问题。
- 仍保留的 P2：导航未标记当前项（同文件 `:732`）；触屏默认仍显示方向键/Ctrl+Enter（`:861`）；“保存筛选为智能列表”仍在结果前占独立整行（`:854`）；内容类型等部分文案仍可进一步中文化。这些不构成当前已确认 P1，不需要扩大本版范围。
- 上次 dump 中 Button 本身名称为空、文字在子节点，仍需新包的 TalkBack/实际可访问性节点确认合并结果。当前通用 action 位于 `/home/czyt/code/rust/yank/crates/yank-client/src/ui/controls.rs:4`；仅凭旧 dump 不认定其一定是未命名按钮，也不宣布已解决。
- `form_feedback` 已有邻近、可读的实际状态文字，但 `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1553` 未显式声明 live status；新包需确认屏幕阅读器是否能获知异步失败。这仍是待验风险，不是凭静态截图新增的已确认 P1。

### 一个仍可收尾的真实反馈 P2

`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1050` 在调用 `copy_clip` 后立即提示“已复制草稿文字”。桌面 copy_clip（`:143`）只是异步发送 Copy 命令，不能在此时证明系统写入成功。后台失败最终会显示错误，因此本轮没有把它升级为 P1，但建议将成功文案延后到真实完成，或先显示“正在复制草稿文字”。保持已有 worker 成功/失败来源即可，不必新建框架。这个建议与原始“反馈真实”的审核范围一致。

### Linux 新截图证据与状态修正

独立查看了 `/tmp/yank-linux-native.png`：可见左侧导航、搜索、空态与详情区域，确认在用户描述的指定环境下已有实际内容渲染。用户说明：原窗口 1120×780，提供的是其中 **1000×600 裁剪**；X100 父 WM frame 先前未映射，手动映射该 app frame 后恢复；运行使用 `GSK_RENDERER=cairo`，隔离目录 `/tmp/yank-native-review` 且 capture=false，没有采集用户剪贴板。

本审核确认的是截图所见渲染；父 frame 原因、启动参数和隔离设置来自用户现场排查说明，未独立重复执行。**此前“X100 仍只有黑屏”的状态被此证据更新**，但右侧详情和底部区域在图中不完整，不能据此评判裁剪边界处的文字为应用截断，也不能宣布完整 1120×780 布局、滚动、焦点、窗口缩放或 Linux 原生全流程通过。

idle CPU 偏高仍由主 agent 排查，本轮没有测量、没有将其归因于环境或 UI，也没有给出性能通过结论。

### 本次给发布判断的明确回答

**原生 UI 当前没有已确认的代码 P1 阻塞；原初审 P1 的对应代码与关键 headless 行为覆盖已具备关闭依据。** 剩余截图层面问题属于上述 P2 或实际可访问性待验。新 APK 的 Android 运行、TalkBack/键盘/返回及真实 arm64 硬件边界仍需保留；Linux 只有当前裁剪渲染证据，完整窗口验收未完成。以上允许区分“代码 P1 已清”和“平台已完整通过”，不将后者提前写成通过。

原生最终静态快照：2026-11-13T05:34:56+00:00。SHA-256 前 16 位：

| 文件 | 摘要 |
| --- | --- |
| `crates/yank-client/src/ui/mod.rs` | `a9811b9b8a086c26` |
| `crates/yank-client/src/ui/controls.rs` | `dd492bf4e03abc70` |
| `crates/yank-client/src/ui/desktop.rs` | `d3dfb5d47f107027` |
| `crates/yank-client/src/ui/theme.json` | `c1aeded415a9aa96` |
| `crates/yank-client/tests/interface.rs` | `64c3e1303a269430` |

## Web 三项 P2 最终定向复测

本轮仅复测用户指定的 **P2-01 / 账号列表旧请求、P2-02 / 密码字段、P2-03 / 字节校验与中文错误**，不扩大到其他 Web、原生或同步功能。**三项均已关闭。** 本节覆盖前文“3 项 Web P2 仍需修改”的历史状态；原生平台与硬件验证边界保持不变。

开始和结束均逐字节核对运行中的 `http://127.0.0.1:17219` 与工作树 HTML/CSS/JS，均一致。使用独立 `yank-ux-audit` 浏览器会话。Nowledge 检索 `Yank Web P2 accounts password bytes audit` 返回 0 条线程。没有修改业务代码，没有创建新账号或实际修改任何账号口令，最终已退出并关闭浏览器。

### P2-01：旧账号请求成功/失败均不会覆盖退出状态——关闭

源码位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:37`、`:53`。loadAccounts 接收 signal 和 generation，提交前比较 generation 与 owner；refresh 的成功/失败显示也检查本次 generation。

复现方法沿用前轮，避免只做静态判断：在管理员账号列表刷新时，浏览器暂缓交付真实 `/api/accounts` 响应，先通过正常 UI 退出，再放行旧结果。

| 场景 | 结果 |
| --- | --- |
| 退出后放行旧成功响应 | status 保持“尚未登录”，online=false，账号缓存 0，空态“先连接你的空间”，session 为 401。旧卡片不再填回。 |
| 退出后放行旧失败 | 在取得真实响应后受控抛出诊断异常；status 仍为“尚未登录”，online=false、账号缓存 0，toast 保持“已退出账号”，没有过期任务的错误提示覆盖。 |

第二项是明确的浏览器故障注入，不描述为服务器自然发生的错误。每次验证后恢复原 fetch；随后 reload，移除全部诊断替身。没有修改服务器响应或服务器数据。

### P2-02：字段尺寸、样式与键盘焦点——关闭

源码位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.css:11`、`/home/czyt/code/rust/yank/crates/yank-server/web/index.html:44`。

- 1440px 浅色：当前/新密码字段均实测 **378×44px**；Tab 从取消按钮进入当前密码字段，焦点轮廓为 2px solid。字段使用 surface/foreground 对应颜色。
- 320px 浅色：两字段均 **226×44px**，dialog 宽 288px；page scrollWidth=320。独立查看截图，标签、全宽输入框、帮助文本、提交按钮均在对话框内，焦点轮廓清楚，未见横向裁切。
- 320px 深色：仍为 **226×44px**；输入背景 rgb(28,36,32)、前景 rgb(227,235,229)，与深色语义 token 对应，键盘焦点轮廓仍可见。

本轮验证的是浏览器几何和样式，没有把 320px 浏览器视口当成 Android 真机软键盘或触摸运行验收。

### P2-03：8–1024 UTF-8 字节校验及中文失败——关闭

源码位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:86`、`:90`；`/home/czyt/code/rust/yank/crates/yank-server/web/index.html:44`。

浏览器对改密新密码和创建账号初始密码两字段分别输入诊断样本并触发真实 input 校验；二者结果一致。原字符数 minlength/maxlength 已移除，使用 TextEncoder 字节长度判断。

| 输入样本 | UTF-8 字节数 | 浏览器校验 |
| --- | --- | --- |
| 7 个 ASCII 字符 | 7 | 拒绝，中文长度提示 |
| 8 个 ASCII 字符 | 8 | 接受 |
| 1024 个 ASCII 字符 | 1024 | 接受 |
| 1025 个 ASCII 字符 | 1025 | 拒绝，中文长度提示 |
| 2 个常用中文字 | 6 | 拒绝，中文长度提示 |
| 3 个常用中文字 | 9 | 接受 |
| 2 个四字节 emoji | 8 | 接受 |

并通过正常表单补测：7 字节新密码点击提交后，浏览器拦截并把焦点置于新密码，customError=true，按钮未锁死。随后用 **故意错误的当前密码** 和合规的 9 字节中文新密码提交真实接口，表单保持打开，错误为“当前密码不正确，或新密码长度不在 8–1024 字节之间。中文通常每字占 3 字节。”，按钮恢复可用。没有成功执行改密，review 主账号口令保持不变。

可见帮助文本为“8–1024 字节；中文通常每字占 3 字节。”，新密码通过 aria-describedby 关联该说明。本轮没有再重复前轮已验证的完整密码生命周期。

### 本轮结论与快照

**指定的 3 项 Web P2 均有修复及针对性浏览器证据，不再保留为未关闭问题。** 本轮不改变其他原生 P2、TalkBack、新 APK、真实 arm64、Linux 完整窗口或同步验证结论。

mDNS/idle CPU 排查由主 agent 负责；本轮没有测量 CPU，也没有将其归因于 UI 或验证容器网卡过滤效果。

运行服务与工作树一致的 SHA-256 前 16 位：

| 文件 | 摘要 |
| --- | --- |
| `crates/yank-server/web/index.html` | `24d3d6e831c0ba88` |
| `crates/yank-server/web/app.js` | `3f52998a0d9693df` |
| `crates/yank-server/web/app.css` | `c48805f527845a3f` |


## 7973ed4 网络 Snapshot 时序修复：定向静态复核

复核日期：2026-11-13 UTC。仅核读用户指定的“数据库未变、网络状态改变时 UI 未刷新”修复，不扩展到同步协议、CPU、其他界面或跨设备联调。目标提交为 `7973ed416f621258242f99acc138b8d923546a68`；复核时工作树 HEAD 为 `ae1667fc304cb7c2c2b4a9c2c203b8a1382d60c4`，worker.rs、ui/mod.rs、network.rs 与目标提交无差异。此处引用当前工作树绝对行号。

**结论：页脚状态、发现设备和服务列表的快照失效链路已补齐，未发现此链路仍存的 P0/P1。设备码文本另有一项直接相关的 P2 订阅遗漏，不能据此称所有网络相关 UI 均已自动刷新。** 本节为静态结论，没有启动浏览器、APK、原生 binary 或执行平台交互。

### 已成立的更新链路

| 环节 | 静态证据与结论 |
| --- | --- |
| 相等比较包含网络状态 | `/home/czyt/code/rust/yank/crates/yank-client/src/worker.rs:13`：Snapshot 派生 PartialEq，新增 network_status、identity、discovered、servers 参与比较；对应结构也支持值比较（`/home/czyt/code/rust/yank/crates/yank-client/src/network.rs:38`）。数据库记录不变已不再意味着 Snapshot 相等。 |
| 周期采样与确定顺序 | `/home/czyt/code/rust/yank/crates/yank-client/src/worker.rs:577`、`:605`、`:612`、`:618`、`:639`：dirty 或距上次采样超过 700ms 时读取网络状态，设备按 peer.id、服务按 url 排序并发送。排序减少 HashMap 遍历顺序带来的比较噪声。700ms 是采样阈值，循环等待及任务执行也会耗时，不是端到端显示延迟保证。 |
| UI 接收和页脚依赖 | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:404`、`:420`、`:431`：接收当前 query/filter 的快照并设置 Signal。页脚在 `:624` 订阅 snapshot，再于 `:633` 读取共享 status；即使 error 内容不变，整个 Snapshot 的变化仍能使该表达式重算。原“离线状态因 DB 相等而滞留”的失效路径在此处已闭合。既有 error/notice 优先展示规则仍适用。 |
| 发现列表依赖 | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1253`、`:1307`：服务列表和发现设备列表显式读取 snapshot Signal，再读取共享 maps，因此网络快照变化能触发列表重新计算。 |
| 行为测试 | `/home/czyt/code/rust/yank/crates/yank-client/src/worker.rs:860`：测试取得初始快照后只改变共享 status 和 servers，断言 clips/stats 未变、服务出现且 Snapshot 不等。覆盖本次主要回归条件；未直接覆盖 identity/discovered 单独改变，也不是 UI 文本显示测试。用户报告新增测试及全量 70 项通过；本轮只核读测试源码，未重跑，不将其记作独立执行结果。 |

### 仍需修改：P2 / 本机设备码未订阅网络快照

- **位置：** `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1201`（读取位于 `:1202`–`:1209`）。
- **条件与影响：** 用户在 identity 尚未就绪时进入“设备与同步”，文本先显示“正在生成…”。随后网络线程在 `/home/czyt/code/rust/yank/crates/yank-client/src/network.rs:406` 写入 identity，worker 即使发送不同 Snapshot，这个文本闭包仍只读取 Mutex，没有读取任何 Signal；因而不会因本次 Snapshot 更新自动重算。本机设备码可滞留初始占位，妨碍展示给另一台设备。切换页面再进入会重建该区域，因此按可恢复的 P2 记录，未升格为整条配对功能不可用的 P1。
- **最小修复：** 在设备码文本闭包中通过 `identity.snapshot.with(...)` 读取 Snapshot.identity 并生成 code/占位文本，使内容与依赖来自同一快照；也可至少显式订阅 snapshot 后保留共享读取方式，前者更直接。
- **依据与验证边界：** 锁定的 NSH 依赖中 `/home/czyt/.cargo/git/checkouts/nsh-9218b667fc8c8325/f4d68f7/crates/nsh-core/src/element/reactive.rs:663`、`:674`、`:706` 明确 get/with 注册订阅，set 只重跑读取该 Signal 的计算；普通 Mutex 不建立此依赖。这是静态确认的订阅遗漏，本轮未在平台上复现占位滞留。建议修复后定向验证“先进入设备页，再让 identity 就绪”的可见文本更新，无须扩大到完整同步验收。

### 发布边界

仅就本次网络 Snapshot 失效修复，没有新确认的 P0/P1 发布阻断项；上述 P2 应补齐后再声称设备码也随快照更新。用户正在更新 APK/原生 binary 的跨设备 UI 联调，本轮没有将其写为完成，也没有将旧 Android 模拟器截图、Linux 裁剪截图或测试通过报告替代新版平台验收。真实 arm64 硬件边界不变。


## Android → Linux 反向同步：有界真实交互验证

日期：2026-11-13 UTC。本轮独占 `emulator-5586`，从主 agent 留下的既有详情和未保存草稿继续操作。使用当前已安装 APK，没有重装、清数据、改账号、改业务源码或直接写数据库。设备仍是此前的 Android 35 x86_64 模拟器环境，真实 arm64 硬件没有验证。Linux → Android 的配对及最初记录到达由主 agent 先行完成，本轮不将其写作本人重新执行。

**结果：通过正常界面保存，Linux 同 ID 正文确实收到追加文本，完成当前构建 Android → Linux 的反向编辑同步证明。本轮未复现“保存编辑”按钮失效，未新增已确认的 P0/P1。**

| 步骤 | 独立观察与证据 |
| --- | --- |
| 操作前确认 | 新取 UI dump 和截图：处于 ID `a1d08176-821b-4a4a-93e1-736fb5c76702` 的记录详情；状态“未保存 · 切换记录或返回后草稿仍保留”，页脚“UI review · 加密中继”。键盘已经收起，“保存编辑”完整可见，因此没有发送 Back 或额外滚动。截图中编辑区在当前滚动位置之外，本轮没有凭截图声称读到草稿全文。 |
| Linux 初值 | 以 SQLite URI `mode=ro` 打开 `/tmp/yank-native-review/yank-v2.sqlite3`，按上述 ID 查询 clips.body 并解析 JSON.text，初值为 `Yank Android relay smoke 2026-11-13`，不含追加标记。未向 DB 写入。 |
| 正常触摸保存 | 通过用户提供的 `/tmp/yank-adb-action.py` 重新获取当前 UI 节点，找到“保存编辑” bounds `[466,1755][614,1808]`，在中心 `(540,1781)` 执行一次同坐标 100ms swipe 作为触摸点击；没有调用应用内部保存接口。 |
| Android 保存反馈 | 保存后重新获取 dump 并独立查看截图：原草稿状态变为“已保存”，页脚仍为“UI review · 加密中继”；留在同一记录详情，应用没有退出。 |
| Linux 反向到达 | 保存后的首次只读查询已读到同 ID text 为 `Yank Android relay smoke 2026-11-13 Android_reply_verified`。随后结束前再次只读确认完全一致。预设的 7 秒轮询未进入等待/超时分支；未采集触摸到 DB 到达的精确端到端延迟，因此不报告亚秒延迟结论。 |

证据文件（本地 `/tmp` 临时产物）：

- 操作前：`/tmp/yank-audit-current.xml`、`/tmp/yank-audit-current.png`。
- 保存后：`/tmp/yank-audit-save-result.xml`、`/tmp/yank-audit-save-result.png`。
- 结束前 Linux 只读核查：`/tmp/yank-audit-linux-reverse-sync.json`，含记录 ID、最终正文、updated 及 UTC 查询时间，不含口令或会话凭据。

主 agent 先前未触发保存的原因尚未证实；本轮一次当前节点中心触摸即成功，不能据此认定原来必然是坐标/手势问题，也没有真实失败证据支持新增 UI bug。验证结束后保持当前已保存详情、键盘收起，不再操作 ADB，交还模拟器使用。此结论只覆盖当前 APK、该隔离测试记录及这次反向编辑同步，不替代完整设备配对、离线恢复、软键盘、TalkBack、新 APK 或真实硬件验收，也不自动关闭前轮静态设备码订阅 P2。


## 最后两处 UI 修复：当前工作树定向静态确认

2026-11-13 UTC。仅检查设备码订阅和设置输入提交方式；没有操作 ADB、浏览器、平台程序或测试，没有改动业务文件。设置改动仍由 Herschel 处理中，本节是当前文件快照结论，不代替后续稳定版本确认。

1. **设备码订阅 P2：源码层面关闭。** `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1208` 现在直接通过 `identity.snapshot.with(...)` 读取 `Snapshot.identity`，从快照生成 code 或“正在生成…”。内容读取与 Signal 订阅一致，前轮 Mutex 裸读导致没有响应式依赖的遗漏已消除。本轮未平台复现 identity 延迟就绪，因此关闭的是代码问题，不记作实际 Android 场景通过。
2. **设置局部草稿和每字段保存：已大部迁移，尚有一处 P2 未完成。** `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1688` 的 pref_text 在 on_change 中只更新 preference_drafts；`:1712` 的独立保存按钮才读取当前已保存偏好、合并该字段并调用 save。未改动/相同值及 pending 状态禁用按钮。设备名、中继、Ditto 字段、排除正则/应用和快捷键已使用此机制。`/home/czyt/code/rust/yank/crates/yank-client/src/ui/controls.rs:123`、`:157` 将保存按钮实际加入字段布局。其他开关保存读取 prefs，不合并尚未提交的文本草稿。

**当前确实仍需改的位置：** `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1756` 的“保留天数（0 为永久）”依旧绑定 prefs，`:1760` 在输入能解析为 u32 时立即设置 retention_days 并在 `:1763` 调用 write.save。输入完整多位数字之前，中间值仍会被提交；清空输入则立即报错，不符合本轮局部 draft + 显式 Save 的目标。最小修复是让该数字输入也保留原始字符串草稿，在对应保存按钮提交时再解析和校验；不要在逐字输入回调中提交。此处记录为剩余 P2，未实测发生清理或数据损失，不将其升格为已确认 P1。

测试源码已核读（未执行）：`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:256` 覆盖未完成正则、快照/开关不覆盖草稿、拒绝后重试及返回保留；`:313` 覆盖文本草稿不随开关提交和分别保存。当前这两项没有覆盖保留天数，不能据此宣称所有可编辑设置字段已迁移。compile/test 正在由主 agent/Herschel 执行，本轮不预先记为通过。

本轮没有新增已确认 P0/P1；设备码 P2 已静态关闭，设置提交方式仍以保留天数这一处为待完成项。不重做既有双向 relay 验证，不改写 platform-verification.md。

本次静态读取文件 SHA-256（前 16 位）：

- `crates/yank-client/src/ui/mod.rs`：`9fc159bd7f0eb116`。
- `crates/yank-client/src/ui/controls.rs`：`50af11378881c538`。
- `crates/yank-client/tests/interface.rs`：`fafc3eb7683ee9eb`。


## 最终限定静态确认与报告冻结

2026-11-13 UTC。按用户要求，只读复核冻结后的 `pref_text` 局部草稿/每字段保存及设备码 Snapshot 订阅，不重做截图、旧测试或平台操作。审核报告在本节补写后冻结，供主 agent 提交。本轮没有修改业务源码、platform-verification.md 或测试数据。

**限定结论：未发现这两处仍存的 P1；设备码响应式订阅 P2 已静态关闭。** 不将静态结论扩大为全平台验收；既有 Android → Linux 反向编辑同步和主 agent 提供的双向 relay 证据保持原边界。

| 指定检查 | 冻结源码证据及结论 |
| --- | --- |
| 输入阶段仅修改局部草稿 | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1688`、`:1701`、`:1706`：pref_text 优先显示 preference_drafts，输入回调只写入该字段草稿；周期偏好快照不覆盖草稿，不在每字输入时提交工作线程。 |
| 每字段显式保存 | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1712`：对应按钮只读取该字段的草稿，基于当前 prefs 合并该字段后 save，不把其他未提交字段一并写入。`/home/czyt/code/rust/yank/crates/yank-client/src/ui/controls.rs:123`、`:155`：按钮被保留并实际加入字段布局。 |
| 按钮禁用和失败草稿 | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1722`：pending、没有草稿或草稿等于已保存值时禁用；草稿在提交时不删除。worker 在 `/home/czyt/code/rust/yank/crates/yank-client/src/worker.rs:341` 的 Save 分支校验，在 `:521` 将结果转为 error 并使快照 dirty；UI 接收快照刷新 prefs（`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:421`），被拒绝的草稿仍可显示并重试。本轮没有发现此路径需要新增 P1。 |
| 设备码订阅 | `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1208`：文本直接通过 identity.snapshot.with 读取 Snapshot.identity，订阅和内容同源，不再裸读 Mutex；原订阅遗漏已修复。 |

新增测试源码的定向核读：`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:256` 包括未完成正则不提交、拒绝后保留草稿且按钮可重试、切页返回保留、成功后按钮禁用；`:313` 包括输入分段/快照不提交、toggle 不携带其他草稿、分别点击保存后持久化。用户报告这两项均已通过；本轮未重跑测试，不将其列为本人新执行的通过结果。

本轮不改变登录输入的提交方式、toggle 即时生效或上传同意流程，也未发现此次 pref_text 改动绕过它们。前轮数字输入“保留天数”仍位于 `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1756`，不使用 pref_text，仍为逐次解析后保存；其既有 P2 记录保留，本次没有将它误写为已迁移，也没有扩大调查或新增 P1。

冻结时 HEAD：`ae1667fc304cb7c2c2b4a9c2c203b8a1382d60c4`，包含未提交 UI 修复；以以下工作树文件摘要界定本次结论：

- `crates/yank-client/src/ui/mod.rs` SHA-256：`9fc159bd7f0eb116d90a1cf3565df87ae5539302d2c20c5dc1e2f523a18294d6`。
- `crates/yank-client/src/ui/controls.rs` SHA-256：`50af11378881c5388a2bbd716c8355b9d97c36fad634afc9f00f555cbb25a11d`。
- `crates/yank-client/tests/interface.rs` SHA-256：`fafc3eb7683ee9eb2c9142405161b7227d9dc79f4667f2aafff49b4192ffffdb`。


## 外观持久化限定补核与最终冻结

2026-11-13 UTC。本次只增加外观持久化的静态核查，并确认 pref_text/identity 修复仍成立；没有扩大新问题、操作 ADB/浏览器、重跑旧测试或更改业务源码。补注完成后本报告冻结，交主 agent 进入 CI。

**结论：外观修复的启动应用、持久化、三态返回和标签链路成立；连同 draft/identity 两处，未发现本次限定范围仍存的 P0/P1。**

- **启动应用保存值：** `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:356` 的 app 根 effect 读取 prefs Signal，将 dark/light/其余值分别映射到 Appearance::Dark/Light/System。初次创建时使用默认偏好，首次 worker 快照到达后 `:429` 更新 prefs，effect 即按已持久化偏好重新应用；不再依赖用户先点击设置按钮。此静态结论不等于已实测冷启动无闪烁。
- **三态与标签一致：** `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1473` 的按钮按 system → light → dark → system 循环，并从同一 prefs.theme 生成“跟随系统 / 浅色 / 深色”标签。标签表达的是用户选择模式；“跟随系统”不伪称当前一定是浅色或深色。按钮调用 Ui::save，worker 在 `/home/czyt/code/rust/yank/crates/yank-client/src/worker.rs:381` 调用 store.save_preferences，主题随偏好保存，后续快照再次驱动 effect。未知 theme 值按 System 显示与应用，点击归一为 system。
- **NSH API 静态核对：** 锁定依赖 `/home/czyt/.cargo/git/checkouts/nsh-9218b667fc8c8325/f4d68f7/crates/nsh-core/src/platform.rs:35` 明确存在 Appearance::{System, Light, Dark}；`/home/czyt/.cargo/git/checkouts/nsh-9218b667fc8c8325/f4d68f7/crates/nsh-core/src/element/ambient.rs:1153` 的 set_color_scheme 接受 Appearance 并提交外观动作。未把 API 存在视为各平台实测通过。
- **既有两项保持：** `/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1216` 仍直接订阅 Snapshot.identity；`:1697` 的 pref_text 仍仅输入局部草稿，由每字段按钮保存，并保留禁用条件和失败草稿。前节结论继续成立，仅行号随外观改动顺延。

未对当前 APK/原生 binary 执行新增外观冷启动、系统主题切换或重启持久化操作；这些行为本节仅有代码链路证据。既有 Android ↔ Linux relay 证据不受本次静态补核影响。没有把 CI 尚待执行的结果提前记为通过。

最终冻结文件 SHA-256（覆盖上一节摘要；报告结论以此工作树快照为准）：

- `crates/yank-client/src/ui/mod.rs`：`4d2000f87d469d764342d678745bb11f69889a235f0249f8e3c08bb0cd092dc3`。
- `crates/yank-client/src/ui/controls.rs`：`50af11378881c5388a2bbd716c8355b9d97c36fad634afc9f00f555cbb25a11d`。
- `crates/yank-client/tests/interface.rs`：`fafc3eb7683ee9eb2c9142405161b7227d9dc79f4667f2aafff49b4192ffffdb`。
