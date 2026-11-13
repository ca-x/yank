# Yank 0.2 独立 UI/UX 初审

审核时间：2026-11-13（UTC）。结论：**当前工作树不应发布**。初审复现了 P0 初始化阻断（报告收尾时工作树已补退出按钮，运行服务仍是旧 HTML，待重新构建/重启复测）；仍存在账号选择不兑现、窄桌面详情不可达、非文字内容保存会覆盖格式等 P1 问题。本次只写本报告，没有修改业务代码、提交或发布。

## 范围、依据与证据边界

审核文件：`crates/yank-server/web/{index.html,app.css,app.js}`、`crates/yank-client/src/ui/{mod.rs,controls.rs,theme.json}`、`crates/yank-client/tests/interface.rs`。为核实上传默认值、登录重置和保存后果，只读追踪了 `crates/yank-core/src/lib.rs` 与 `crates/yank-client/src/worker.rs`。

已读取并应用本地 `ui-ux-pro-max`、`apple-design`、`animate`、`nsh-design-guides`（包括 `references/design-guides.md`）和 `agent-browser` skills；动效 skill 仅作为审核准则，没有执行实现流程。依据重点为 NSH 的 Responsive windows、Interaction states、Feedback and overlays、Motion、中文文案及 Accessibility checklist，以及高频/键盘操作无需动效的约束。

证据类型：

- **代码确认**：可以由当前源代码直接推导；未把原生平台行为描述为实测。
- **浏览器实测**：独立 `--session yank-ux-audit`，本地 Chromium，`http://127.0.0.1:17219`。初次加载时，服务返回的 HTML、CSS、JS 均与当时工作树字节一致。报告收尾再次比对时，工作树已增加退出按钮，运行服务的 HTML 与工作树不一致，CSS/JS 仍一致；以下浏览器结论对应初审旧 HTML，不代表已验证最终修复。检查了 1440×1000、390×844、320×720，浅色及深色的部分状态；用截图查看了桌面和 390px 空状态，用 axe 4.12.1 检查了可达状态。
- **受阻/未验证**：正常登录入口受 P0 阻断，未登录、未创建或删除账号、未修改服务器记录，也未执行真实配对或上传。为单独检查弹窗 DOM，曾在浏览器中直接调用 `showModal()`，这只是诊断，不算登录路径通过。Android 真机/模拟器、原生桌面窗口、原生屏幕阅读器、软键盘、真实慢网和后台任务均未运行。未执行 Rust `interface` 测试；下面对测试文件的判断属于静态审查。

Nowledge CLI 定向检索：`nmem --json t search 'Yank 0.2 UI pairing upload'`，返回 0 条线程；没有据此推断历史决定，也没有写入记忆。没有将测试凭据写入报告。

等级定义：P0 为核心界面普遍不可用；P1 为发布前必须修复的数据、知情同意、核心操作或可访问性问题；P2 为明确需要改善的状态、排版或覆盖缺口。审核期间其他 agent 正在并行扩展及格式化原生代码；本报告在写入结束前重新核对了关键 P1 问题仍然存在，并将原生文件引用更新到当时最新行号；UX-01 已出现代码修复候选，另行标注。行号对应该时点未提交工作树，CSS/JS 当前多为整行压缩，故同时给出函数或选择器便于定位。

## P0

### UX-01：缺失退出按钮使管理页初始化终止，三个登录入口也全部报错（已出现修复候选，待实测）

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:19`（`$("#logout").onclick=…`）、同文件 `:15`（`openSettings`）；`/home/czyt/code/rust/yank/crates/yank-server/web/index.html:43`（弹窗没有 `id="logout"`）。
- 证据：**代码确认 + 浏览器实测**。直接调用登录入口处理函数得到 `TypeError: Cannot set properties of null (setting 'hidden')`，栈定位 `app.js:15:149`。初始化时第 19 行也会对 null 赋 `onclick`，因而第 20 行及之后不执行。浏览器检查 `#refresh/#search/#copy/#pin/#delete/#create-account` 与所有 `[data-view]` 的对应处理器均为 null，只有前面注册的主题和登录入口有函数。
- 状态更新：**并行工作树已在 index.html:43 补上退出按钮**；末次 HTTP 比对显示运行服务仍缺此按钮。初始证据中的“弹窗没有 logout”描述的是首次加载快照。这个缺失 DOM 的直接根因已有代码修复候选，本次未重启他人服务，也未将它标为实测关闭。
- 影响：初审运行页面看起来完整但主要按钮无效；登录框打不开；现有会话恢复也未执行。不能把后续界面截图当成工作流通过。
- 最小修复：补齐有清楚标签的退出控件，与 JS 的 DOM 契约一致；可选元素再做显式存在性处理。重新加载后用正常 UI 完成登录、刷新、导航、复制、退出，并确认无未捕获异常。

## P1

### UX-02：“不关联账号”在同一服务器上仍保留账号与上传，违背明确选择

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1001`（设置页“仅使用发现与中继，不关联账号”）、`:803`（服务发现中的同名选择）。
- 证据：**代码确认**。两处都仅在服务器 URL 改变时清空 token、account_id、username 并关闭 upload。同一 URL 下点击“不关联账号”仅保存原设置，原先登录和 upload=true 可以原样保留。保存处理未重新解释此选择，见 `/home/czyt/code/rust/yank/crates/yank-client/src/worker.rs:230`。
- 影响：已登录且启用上传的用户明确选择“仅中继、不关联”，仍可能继续账号上传。这是知情同意与实际行为不一致，不能仅靠改提示掩盖。
- 最小修复：无论 URL 是否改变，选择此项均清除本机账号关联并关闭上传；在完成后显示真实新状态。复审覆盖同一服务器、不同服务器、upload 开/关组合。若产品意图只是保持已有账号，则必须移除“不关联账号”承诺，并另设真实可用的解除入口。

### UX-03：桌面窗口小于 850px 后，选中记录无法打开详情

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:212`（`desktop` 使用宽度与平台）、`:532`（仅 desktop 显示详情）、`:46`（`choose` 只按 `!is_desktop()` 跳详情）、`:547`（记录按钮）。
- 证据：**代码确认；未运行原生窗口**。桌面平台宽度低于 850px 时，右侧详情被隐藏，但点击记录不会切到 detail 页面。布局判断与导航判断采用不同条件。
- 影响：窄窗口/桌面分屏下无法通过记录入口访问编辑、收藏、队列、发送及删除等详情操作。
- 最小修复：使用同一个响应式条件决定“侧栏详情还是单独详情页”；窄桌面选中即进入详情，并提供明确返回及焦点恢复。验证 849/850px 和窗口缩放过程中已有选中项。

### UX-04：图片和文件也能“保存编辑”，会被无提示替换成纯文字

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:630`、`:659`；结果处理 `/home/czyt/code/rust/yank/crates/yank-client/src/worker.rs:215`（`Command::Edit`）。
- 证据：**代码确认；未执行破坏性操作**。详情无条件提供编辑文字和保存按钮；保存无条件将 `formats` 替换为单个文字格式，并设 `kind="text"`，即便原记录是图片/文件，或用户只改收藏板名称。
- 影响：用户按普通“保存编辑”会丢失原条目的二进制/多格式内容，并可能同步该修改；界面没有说明这是格式转换。
- 最小修复：非文字条目先禁用文字保存并解释原因；将收藏板元数据保存与内容替换分开。确需转文字时作为另一个明确操作，说明覆盖范围并保留原条目。

### UX-05：Web 全局 Enter 劫持有焦点按钮，执行结果与按钮标签不一致

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:34`；记录按钮生成 `:24`；底部快捷键说明 `/home/czyt/code/rust/yank/crates/yank-server/web/index.html:37`。
- 证据：**代码确认；受 UX-01 阻断，未称实测**。只排除 input/textarea/select；当已有选中项时，任何 button/link 上的 Enter 都 `preventDefault()` 并调用 copy。Tab 到“收藏”“删除”“加载更多”后按 Enter，也会复制之前选中的条目。上下键修改 selected，但没有把键盘焦点移到选中记录。
- 影响：键盘用户无法信任焦点与按键结果；视觉选中项和实际焦点可以分离。
- 最小修复：把列表快捷键限定于列表焦点上下文，尊重按钮/链接默认激活及修饰键；方向键同步可见焦点。保留显式复制快捷键时应与 Enter 激活当前按钮区分。

### UX-06：窄屏 Web 详情出现在完整列表之后，选择条目不带用户到详情

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.css:4`（max-width:900px 的 `.library-layout` 与 `.inspector.open`）；`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:22`、`:23`、`:28`。
- 证据：**代码确认；有数据交互未实测**。窄屏改成单列，详情在 history-panel 后，仅切换 open；没有滚动到详情、移交焦点或页内导航。列表每页最多 100 条（`:25`），详情可能位于数千像素之后。关闭详情又 `render()` 替换全部记录按钮，也没有恢复触发条目的焦点。
- 影响：手机上点选看似没有反应；键盘/屏幕阅读器用户没有进入详情和返回列表的可靠位置。
- 最小修复：窄屏使用独立详情页或合适的详情层，明确返回；至少确保选择后详情标题获焦并可见、关闭后焦点回到原条目。避免为了关闭详情重建整个列表。

### UX-07：删除传播到设备，却没有预先说明范围，也没有撤销

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/index.html:36`；`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:30`、`:31`；`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:669`。
- 证据：**代码确认；未执行删除**。Web 直接 DELETE，只有完成后才说“删除状态会同步到设备”；原生直接发送 Delete 并清空编辑区。两处没有事前范围提示、确认或撤销入口。
- 影响：用户无法在操作前判断是删除本地副本、服务器副本还是同步条目；误操作后没有界面恢复路径。
- 最小修复：优先提供可兑现的撤销；暂时没有撤销能力时，用简短确认展示条目名称与同步后果。让危险操作的标签、样式和范围一致，避免给高频非破坏操作增加确认。

### UX-08：浅色侧栏正文及分组文字对比度不达标

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.css:1`（`--muted/#66746c`、`--rail/#f0f2ef`），`:3`（`.brand-sub/.workspace small/.badge/.section-label`）。
- 证据：**浏览器实测，axe 4.12.1**。桌面浅色检测到上述 5 个节点对比度约 **4.35:1**，低于这些小字号文字所需的 4.5:1。品牌副标和 SERVER 为 8px，自托管说明为 10px，资料库/内容类型为 11px。深色 320px 可达空状态扫描没有已确认 violation，但仍有 incomplete，不能推为全站通过。
- 影响：低视力用户和缩放阅读受影响，中文分组与说明难辨认。
- 最小修复：调整 muted 在 rail 上的语义配色，留出超过 4.5:1 的余量；关键中文说明提高到可读字号。不要把 axe 对空列表的 incomplete 当作另一个已确认错误。

## P2

### UX-09：原生登录与长任务缺少可见 pending 状态，错误离输入太远

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:12`、`:253`、`:1000`、`:664`、`:1023`、`:1024`；`/home/czyt/code/rust/yank/crates/yank-client/src/ui/controls.rs:4`。
- 证据：**代码确认**。Ui 没有每项任务的 pending 状态；登录立即清空密码，但按钮持续可用，界面没有“正在登录”。后台登录请求可等待至 12 秒（worker.rs:256）；Ditto 发送可至 40 秒（worker.rs:315）。错误主要在页底 text_xs 中呈现，既未靠近字段也未声明 live status/alert；文件选择的 Err 被 `if let Ok` 静默忽略。
- 影响：用户可能重复提交，无法区分正在处理和未响应；屏幕阅读器不一定获知新错误，软键盘下页底也可能不可见。不能只通过视觉禁用假装任务结束。
- 最小修复：为登录/导入/发送等真实异步任务显示即时状态和防重复提交；把字段错误贴近字段并关联，把整体状态置于可宣告区域；取消与失败分开反馈。不得虚构进度百分比。

### UX-10：互斥传输模式看不出选中项，选项语义不完整

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:963`、`:1011`；`/home/czyt/code/rust/yank/crates/yank-client/src/ui/controls.rs:4`。
- 证据：**代码确认**。三个模式都由相同的 `action(..., false, ...)` 渲染，没有绑定 `prefs.mode` 的选中样式或 checked/pressed 语义。
- 影响：用户点选后无法从控件判断当前是中继、直连还是自动；屏幕阅读器也没有选中项信息。
- 最小修复：使用有组名的互斥选择控件，绑定真实 prefs.mode，并为选中、焦点、禁用提供独立状态；窄屏允许纵排长选项。

### UX-11：手机导航中文被挤断，隐藏内容类型的 CSS 又被更高特异性覆盖

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.css:4`（`.format-nav{display:none}` 与后续 `.sidebar>nav{display:flex}`、`.nav-item`）；`/home/czyt/code/rust/yank/crates/yank-server/web/index.html:21`、`:26`。
- 证据：**浏览器视觉实测**。390px 截图中“全部记录”拆成“全部记 / 录”；320px 下三个主导航各 92px 宽、56px 高，文字/计数挤成多行。`.format-nav` 计算样式仍为 flex，因 `.sidebar>nav` 特异性更高。没有发现这两个宽度上的整页横向溢出（scrollWidth 等于 innerWidth）。
- 影响：中文阅读节奏和导航密度不稳，手机首屏大量空间被导航与宣传标题占用，记录列表起点偏低。
- 最小修复：按真实意图明确保留一行可读导航或收进类型菜单；修正选择器作用域；给主导航标签合理的不可拆分范围，计数可在窄屏隐藏。无需用更小字号解决。

### UX-12：原生详情缺少条目类型与草稿状态的清楚反馈，容易误以为复制的是编辑内容

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:39`、`:54`、`:630`、`:647`、`:659`、`:205`。
- 证据：**代码确认**。编辑更新 draft，但“复制”读取 snapshot 中的原记录；切换条目或 Escape 直接离开，不提示未保存草稿。详情还将内部 kind（如 text/files）直接展示给中文用户。
- 影响：用户看到新文字按“复制”，实际拿到旧文字；离开后编辑丢失，缺少可解释状态。
- 最小修复：明确“复制原文/复制编辑内容”的对象，呈现未保存状态；有变更时提供保存、保留草稿或舍弃路径；把类型映射为一致中文标签。

### UX-13：测试名声称 mobile，但没有证明移动平台、同意或焦点路径

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:4`、`:16`；共用启动 `:5`、`:17`。
- 证据：**测试代码审查，未执行**。两项都用默认 EngineConfig，没有显式 Android 平台、窗口尺寸或软键盘条件；断言仅验证部分文案及页面切换。不能据此宣称 Android 布局、触摸、焦点、同意语义均通过。
- 影响：UX-02/03/04 等核心错误可以在“界面测试通过”的印象下遗漏。
- 最小修复：增加少量有行为意义的回归：同服务器解除关联会关闭上传；849px 桌面点记录可进入详情并返回；图片/文件不被普通保存变成文字；键盘激活当前按钮；默认上传关闭以及取消上传确认不改变设置。Android 另做明确目标运行与软键盘/系统返回验收，无需镜像每个控件写测试。

## 当前值得保留的设计与未发现的动效问题

- 上传默认关闭：`/home/czyt/code/rust/yank/crates/yank-core/src/lib.rs:339`；登录成功又重置为 false：`/home/czyt/code/rust/yank/crates/yank-client/src/worker.rs:272`。UI 启用前要求登录并显示确认，明确“本机历史及后续修改上传”“关闭仍保留服务端数据”（mod.rs:1004–1010）。这部分代码承诺清晰，但没有运行真实传输，不能据此证明所有网络流量符合承诺。
- 设备配对与账号关联在按钮选择上保持独立；接收邀请还允许拒绝和“仅设备互通”，账号关联随后进入登录页，没有把配对等同于自动登录（mod.rs:140–190、:757、:913–925）。应在最终配对实测时再核对邀请人/服务器身份和双端取消，不应因尚未实测直接宣布完成。
- 原生有 app-owned controls、浅深色语义 tokens、焦点轮廓，移动 action 最小高度 48；Web 有 focus-visible 和搜索 focus-within，移动多数按钮至少 44px。
- 未见给复制、搜索、列表选择等高频操作添加装饰性动画；Web 仅偶发登录弹窗有 160ms opacity/translate 进入，并在 prefers-reduced-motion 下关闭。无需为“更像 Apple”添加高频动效。本次没有把不存在的动效缺陷列入问题。
- Web 复制成功通知只在 clipboard.writeText resolve 后显示，失败有明确通知（app.js:29）；这段代码本身反馈时机合理，但受 UX-01 影响尚未实测。

## 发布门槛与复审建议

发布前必须关闭的门槛：**UX-01 至 UX-08**。UX-01 已有代码修复候选，属于待重启服务并实测关闭的门槛；UX-02 至 UX-08 在末次静态核对仍成立。其中 UX-01 先恢复可测试的完整 Web 工作流；UX-02 保证用户明确选择“不关联”会停止账号上传；UX-03/04 恢复核心详情可达及内容完整性；UX-05/06/08 修复键盘、窄屏访问和已实测对比度；UX-07 补齐删除前范围及恢复/确认。

之后复审应针对最终变更运行：正常登录与退出；浅/深色列表与详情；Web Tab/Enter/方向键/Escape 和关闭后的焦点；390px 长列表详情；桌面 849/850px；Android 实际目标的输入、软键盘和系统返回；配对双端拒绝/仅互通/邀请关联；同 URL 和不同 URL 的解除关联；上传默认/取消确认/关闭后的真实状态；失败时不显示成功。所有未运行平台继续标为待验，不由截图或 headless 文案断言代替。

## 引用快照

最终行号核对时间：2026-11-13T04:24:48+00:00。并行工作可能继续移动行号；以下 SHA-256 前 16 位记录末次核对的工作树；其中 HTML 已包含 UX-01 修复候选，区别于浏览器初次加载的旧 HTML。

| 文件 | SHA-256 前 16 位 |
| --- | --- |
| `crates/yank-server/web/index.html` | `d300d4c0db531d91` |
| `crates/yank-server/web/app.css` | `93714e459e474e0d` |
| `crates/yank-server/web/app.js` | `543ec36797f48b92` |
| `crates/yank-client/src/ui/mod.rs` | `e813f0a8442cf490` |
| `crates/yank-client/src/ui/controls.rs` | `4755dc93c24c1567` |
| `crates/yank-client/src/ui/theme.json` | `c1aeded415a9aa96` |
| `crates/yank-client/tests/interface.rs` | `9a0a7e3604afcab7` |
