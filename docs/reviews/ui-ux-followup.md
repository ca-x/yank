# Yank 0.2 UI/UX 复审

日期：2026-11-13 UTC。范围：当前未提交工作树中 UX-01～13 的修复，以及重建后的本地管理页。**当前仍有需要修改的键盘/焦点 P1；未发现仍成立的原 P0。不能据本报告宣布整版或原生平台验收通过。**

本版按 **Yank 自有同步** 审核；**微信输入法不在本版范围**，也不是本报告的发布要求。原生同步由主 agent 继续处理。本审核没有修改业务代码、重启服务、提交或发布，仅写入本报告。

## 依据与验证范围

沿用已读取的 ui-ux-pro-max、apple-design、animate、nsh-design-guides 及其 `references/design-guides.md`；浏览器使用 agent-browser 独立会话 `yank-ux-audit`。本次执行 Nowledge 定向检索 `nmem --json t search 'Yank UX-01 UX-13 UI audit'`，返回 0 条线程。

- 静态检查：Web HTML/CSS/JS、原生 `ui/mod.rs`、`controls.rs`、theme、`tests/interface.rs`，并追踪 worker 的 Edit/Save/登录/长任务处理、核心 `detach_account` 及其新增测试。
- 浏览器：用户通知重启后访问 `http://127.0.0.1:17219`。开始时逐字节核对 HTML/CSS/JS，三者与当前工作树均一致。运行了正常登录、弹窗 Escape、真实退出、会话失效查询、错误密码、桌面/手机宽度、浅/深色可达状态的 axe 扫描。尺寸为 1440×1000、390×844、320×720；查看了 390px 截图。
- 账号没有服务器记录。列表键盘与手机详情采用**仅注入页面内存的 A/B 两条诊断样本**；没有写入服务器。复制函数的浏览器 clipboard.writeText 被替换为记录参数并 resolve，故相关结果验证“应用传了哪条内容”，**没有验证真实系统剪贴板写入**。随后 reload 已移除诊断样本与替身。
- 没有运行 Android 真机/模拟器、原生桌面窗口、原生屏幕阅读器、软键盘或真实设备同步/配对/上传。没有运行 Rust 测试或构建。原生项目的“静态修复”只说明代码已覆盖原问题，不等于平台通过。浏览器桌面尺寸也不代表原生桌面验收。
- 测试凭据未写入报告；没有创建、删除服务器记录或账号。错误密码仅用于失败反馈自测。

## UX-01～13 修复核对

| 原编号 | 结论 | 依据与待验项 |
| --- | --- | --- |
| UX-01 初始化终止 | **原 P0 已关闭：代码及正常登录实测** | `index.html:43` 已有退出按钮，JS 引用的静态 DOM ID 未发现缺失。正常 UI 登录成功，账号状态与导航载入；不再出现初审缺失元素导致的初始化阻断。退出后的状态显示另见 F-03。 |
| UX-02 不关联仍保留账号/上传 | **静态修复，原生及同步待验** | `mod.rs:1113` 和 `:937` 均无条件调用 `detach_account()`；`yank-core/src/lib.rs:876` 清空 token/account_id/username 并将 upload=false。`tests/replication.rs:247` 新增同服务器回归用例，本次未执行。 |
| UX-03 窄桌面详情不可达 | **静态修复，原生待验** | `mod.rs:79` 使用 width<850 或非桌面打开详情，与 `:268` 的侧栏条件对应。需实际 849/850px、缩放与返回路径验收。 |
| UX-04 图片/文件被保存为文字 | **原破坏性路径静态修复，原生待验** | `mod.rs:751` 对 image/files 只读；`worker.rs:252` 仅非 image/files 且文字改变时替换文字格式，收藏板单独更新，不再无条件覆盖图片/文件 payload。仍应做图片/文件修改收藏板后格式保留的行为回归。 |
| UX-05 Enter 劫持与焦点不同步 | **部分修复，仍有 P1：F-01** | 普通功能按钮已不再被 Enter 全局复制分支拦截，方向键末尾也有 focus。但 Tab 到另一记录按 Enter 仍复制旧选中项。 |
| UX-06 手机详情与关闭焦点 | **部分修复，仍有 P1：F-02** | `select()` 在窄屏聚焦 `detail-title` 并滚动；HTML 标题增加 tabindex=-1，点击关闭恢复原条目焦点，390px 诊断样本已验证这些分支。Escape 仍走旧 render 路径并丢焦点。 |
| UX-07 删除前范围/恢复 | **静态修复，未执行删除** | Web `app.js:31` 增加删除前 confirm，说明传播范围；原生 `mod.rs:790` 增加带后果描述的 destructive 确认。没有把真实删除与传播标为实测通过。 |
| UX-08 浅色文字对比度 | **原报告中的配色缺陷已修复；可达页面实测未复现** | CSS `--muted` 从 #66746c 调为 #57665e；对 #f0f2ef 的计算对比度约 5.38:1。1440px 浅色/深色可达页面 axe 均 0 个 confirmed violations，仍有 incomplete；不代表所有内容/主题均无障碍合格。 |
| UX-09 长任务/错误反馈 | **部分修复，P2 仍存：F-04** | 已有 busy 信号和提交拦截；密码在登录成功后清空。但 busy 未用于可见文案或按钮状态，错误仍在页底，文件选择错误仍静默丢弃。 |
| UX-10 传输模式选中状态 | **未修复，P2：F-05** | 三个按钮仍都使用相同非 primary action，没有 checked/pressed/selected 与 prefs.mode 的读取。 |
| UX-11 手机导航断行 | **针对已测宽度修复** | CSS 第 9 行覆盖正确作用于 format-nav，隐藏计数并约束主导航。390px 截图中文单行；320px 主导航按钮各 92×44px、format-nav 为 none、页面 scrollWidth=320。没有 Android 平台结论。 |
| UX-12 草稿与复制对象 | **部分修复，P2：F-06** | 文案已改“复制已保存内容”，消除复制对象歧义；仍无未保存状态/离开保护，kind 仍直接显示内部英文。 |
| UX-13 测试覆盖 | **部分补充，平台与关键交互回归仍缺** | 新增核心 detach_account 同服务器测试；原生 interface.rs 两项仍为默认 EngineConfig 和文案/导航断言，未显式设置 Android/849px，也未覆盖草稿、焦点和内容保存保护。不能因测试名含 mobile 认定 Android 通过。 |

表内短文件名均相对于 `/home/czyt/code/rust/yank/crates/`：Web 位于 `yank-server/web/`，mod.rs 位于 `yank-client/src/ui/`，worker 位于 `yank-client/src/`。下方所有需要修改的发现均给出绝对位置。

## 仍需修改的 P1

### F-01 / UX-05：Tab 到记录 B 按 Enter，复制的仍是记录 A

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:34`（全局 keydown）、`:29`（copy 读取 selected）、`:24`（条目只有 click 更新选中）。
- 代码问题：新 interactive 过滤保护了普通按钮，但“Enter 且焦点在 clip-row”分支直接 copy()，没有用当前焦点条目的 data-id 更新选中或确定复制对象。Tab 只移动焦点，不触发 select。
- 浏览器诊断复现：内存样本 A/B → 点 A → Tab 到 B → Enter。观测到 `activeElement.dataset.id="audit-b"`、详情仍为“审核样本 A”，复制调用参数为 `AUDIT_A`。这是参数采集实测，未声称真实系统复制完成。
- 影响：用户依据可见焦点执行复制，却拿到另一条剪贴板内容；复制对象错误是核心操作问题。
- 最小修复：以获得焦点的条目 ID 执行 Enter 操作，确保焦点、选中、预览、复制对象一致；或让 Enter 按钮默认激活选择，另用明确快捷键复制。不要恢复对其他功能按钮的全局劫持。
- 验收：选择 A，Tab 到 B，Enter 的对象必须为 B；Shift+Tab、方向键后 Enter 同样正确；Tab 到收藏、删除、关闭等按钮仍执行其自身操作。

### F-02 / UX-06：Escape 关闭详情仍重建全部记录，键盘焦点落回 BODY

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:34`（Escape 分支）、`:28`（点击关闭分支）、`:24`（render 替换列表）。
- 浏览器诊断复现：选中 B 后按 Escape，详情隐藏，但 `document.activeElement.tagName="BODY"`。相比之下点击关闭可恢复到 B；390px 选择后标题获焦、点击关闭回原条目的修复也已验证。两种关闭方式不一致。
- 影响：键盘用户关闭详情后丢失列表位置，下次 Tab 从非预期位置开始；长列表中会打断任务连续性。
- 最小修复：抽取一致的关闭处理，Escape 与关闭按钮均不替换列表，并恢复原条目焦点。关闭时还要同步清掉旧 aria-pressed 和 selected 样式：当前点击关闭只 renderDetail，诊断发现两行 aria-pressed 仍为 `[false,true]`，与已关闭选择不符。
- 验收：桌面/390px 下按 Escape 与点击关闭均返回原条目，读屏状态与视觉选择同步；登录弹窗 Escape 继续优先关闭弹窗且返回触发器。

## 新增真实反馈问题

### F-03 / P2：退出成功后仍显示“已登录”和在线，空状态也未恢复

- 位置：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:19`（logout）、`:24`（render 只有登录分支更新空态）、`:25`（登录状态展示）。
- **真实服务实测**：正常登录后经设置按钮退出，toast 为“已退出账号”，弹窗关闭，随后只读 `/api/session` 查询返回 401；但页顶仍是“review · 已登录”，connection-dot 仍 online，按钮仍“连接设置”，空状态仍“空间还很安静 / 在客户端启用上传后…”，未恢复连接入口。
- 影响：账号状态与已完成操作矛盾，用户无法判断会话是否撤销。这里没有发现“退出后仍能读数据”的证据，不能将显示问题夸大成会话撤销失败。
- 最小修复：退出完成后集中重置状态文案、在线标记、连接按钮、导航标题/选中及登录前空态；render 给未登录状态完整分支。不要让页面文案成为已失效会话的残留。
- 验收：退出 toast 后所有界面状态立即一致，session 为 401，三处连接入口可重新打开登录；不需要 reload 才恢复。

## 仍需修改的 P2

### F-04 / UX-09：busy 状态没有接到界面，错误仍可能被忽略

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:44`（设置 busy）、`:154`（同步 busy）、`:311`（页底只读 error/notice/status）、`:1112`（登录按钮）、`:1135`、`:1136`（文件对话框）。
- 进展：长任务重复提交会被拦截，密码不再提交即清空。仍缺：没有可见组件读取 busy 信号，用户不会看到“正在登录/导入/发送”；提交按钮持续可用，只有第二次按下才告诉用户等待。页底错误仍不是字段错误或 live region；文件对话框 Err 被忽略。
- 最小修复：用现有 busy 信号驱动真实进行中标签/不可重复提交状态，并宣告状态；登录错误放在账号表单附近；文件对话框区分取消和失败。无需添加虚构进度、装饰动效或新的任务框架。

### F-05 / UX-10：互斥模式仍没有选中语义与视觉状态

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:1075`、`:1083`；`/home/czyt/code/rust/yank/crates/yank-client/src/ui/controls.rs:4`。
- 三个选项只写 prefs.mode，呈现不读当前值。用户不能从按钮判断正在使用哪种模式。
- 最小修复：用互斥选择组绑定 prefs.mode，或等效提供组名、选中语义及可见样式；长选项在窄屏可纵排。模式是否真实建立对应链路由同步实现另验，不从 UI 代码宣布通过。

### F-06 / UX-12：未保存草稿仍可静默丢弃，类型名称没有本地化

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/src/ui/mod.rs:74`（选记录覆盖 draft）、`:253`（Escape 直接离开）、`:741`（kind）、`:748`（改 draft）、`:766`（已保存内容文案）、`:778`（保存）。
- “复制已保存内容”已准确，值得保留。但切换记录/离开详情没有脏状态或保存/舍弃路径；类型继续直接显示 text/image/files 等内部字符串。
- 最小修复：比较草稿与已保存值，显示未保存状态并在离开时保留草稿或让用户选择保存/舍弃；仅对实际修改提示，避免给常规导航加确认。类型用中文映射。

### F-07 / UX-13：回归应集中覆盖本次实际失败分支

- 位置：`/home/czyt/code/rust/yank/crates/yank-client/tests/interface.rs:4`、`:16`；`/home/czyt/code/rust/yank/crates/yank-core/tests/replication.rs:247`。
- 当前新增测试只证明 detach helper 的逻辑，尚未证明两处 UI 路径、849px 详情、格式保存、取消上传确认、任务反馈或 Android 行为。原生测试没有明确平台配置。
- 最小补充：优先为 F-01/F-02 添加能识别焦点与复制对象的交互回归；为图片/文件保存和相同服务器解除关联保留行为回归。Android 的系统返回、软键盘、触摸与文本大小必须实际目标验收，不把 headless 文案测试当平台证明。

## 已完成的浏览器检查与实际限制

| 检查 | 结果 | 边界 |
| --- | --- | --- |
| 正常登录 | 正常路径成功，账号状态与账号管理入口出现 | 使用本地已有测试账号，无凭据持久化到报告 |
| 登录弹窗 Escape | 关闭并将焦点恢复到 connect-top | 本地 Chromium 的 HTML dialog 行为 |
| 错误密码 | 错误提示“请检查用户名和密码，或重新登录。”，表单保持打开，提交按钮恢复 | 未伪造网络成功；不代表全部网络错误均验收 |
| 正常退出 | 请求成功，session=401，toast 正确 | 显示仍错误，见 F-03 |
| 浅/深色 contrast | 两次 axe 扫描均 0 个 confirmed violations，原浅色侧栏问题未复现 | 空 list 与少数字形有 incomplete；不代表完整 WCAG 合规 |
| 320px/390px 排版 | 320px 主导航单行、44px 高，隐藏的类型栏不再被 CSS 覆盖；390px 截图无原中文挤断 | 未测系统大字、缩放、软键盘或 Android 原生布局 |
| 方向键与点击关闭 | A/B 诊断样本方向键可将焦点与选中移到 B；点击关闭回原条目 | Enter 与 Escape 仍失败，见 F-01/F-02 |
| 手机详情 | 390px 点击诊断样本后详情标题获焦、详情区域可见；点击关闭回原条目 | 两条内存样本，未称 100 条真实长列表或所有焦点路径通过 |
| 高频操作动效 | 未发现新增复制/搜索/选择动画；登录弹窗仍可 reduced-motion 关闭动效 | 没有要求添加高频动效，无原生动效实测结论 |

## 当前发布判断

原 P0 的缺失 DOM 与初始化阻断已通过正常登录路径复核关闭。**F-01（焦点 B 却复制 A）与 F-02（Escape 丢失列表焦点）仍需修改并复测，暂不建议发布。** F-03 是已验证的退出状态反馈缺陷，应一并修正。

UX-02/03/04/07 等原生问题目前只到静态修复；不能以此宣布自有同步、Android 或原生桌面验收通过。修复下一轮应优先重跑上述少量失败分支，再补原生目标验证，不需要重复扩大无关设计范围，也不加入微信输入法要求。

## 最终核对快照

核对时间：2026-11-13T05:06:15+00:00。原生工作区正在并行修改，后续行号可能移动；以下为该时点文件 SHA-256 前 16 位。

| 文件 | 摘要 |
| --- | --- |
| `crates/yank-server/web/index.html` | `5474c5c74c34dc0f` |
| `crates/yank-server/web/app.js` | `b534bf62f98dff8d` |
| `crates/yank-server/web/app.css` | `0fc660e145fc8ae8` |
| `crates/yank-client/src/ui/mod.rs` | `5f6333bca53ca7a7` |
| `crates/yank-client/src/ui/controls.rs` | `dd492bf4e03abc70` |
| `crates/yank-client/src/worker.rs` | `7872b075a3cb3f5d` |
| `crates/yank-client/tests/interface.rs` | `9a0a7e3604afcab7` |
| `crates/yank-core/tests/replication.rs` | `f127dc5ec96aae0a` |

## 增量静态复核：改密功能开发期间

核对时间：2026-11-13T05:08:21+00:00。用户说明 Web 源码正在增加改密/管理员重置密码，17219 仍为前次构建；本轮没有把旧服务当作新功能的实测，也没有再次操作浏览器或修改账号密码。仅核对既有 Web UX 的源码状态，原生 P2/草稿由 Herschel 修改，待完成通知后独立复审。

- **F-01 / P1 仍成立**：`/home/czyt/code/rust/yank/crates/yank-server/web/app.js:34` 的 Enter 分支仍以旧 selected 调用 copy，尚未绑定当前焦点记录的 data-id；前次“焦点 B、参数 A”的实测结论未被当前源码修复。
- **F-02 / P1 仍成立**：同文件 `:34` 的 Escape 仍清 selected 后 render，替换列表；`:28` 的点击关闭虽然恢复焦点，仍没有同步清掉原条目的 selected/aria-pressed。关闭路径尚未统一。
- **F-03 / P2 仍成立**：同文件 `:19` 的退出处理尚未清除已登录文案、在线圆点和连接按钮状态；render 的未登录空态也未补齐。前次已确认会话确实撤销，问题仍是界面残留，不升级为会话安全结论。
- **UX-01 原缺失 DOM 问题未再出现**：静态扫描 JS 中直接引用的 `$("#id")` 与当前 HTML，未发现缺失 ID。这只是 DOM 契约检查，不能替代新构建初始化、改密表单或重置密码实测。

当前 Web app.js SHA-256 前 16 位：`924bc02c487a3f24`。本节增量优先于上方旧快照；原浏览器证据仍归属于前次已比对的构建。

最终浏览器轮次在重建通知后执行：重新比对服务与工作树，复测 F-01/F-02/F-03；随后检查本人改密及管理员重置的表单标签、验证、提交中状态、失败反馈、成功后的会话提示、Escape/焦点恢复及桌面/手机布局。任何实际改密或重置只使用获准的本地测试账号，并恢复必要测试状态，不在报告写入凭据。不会将尚未部署的新表单或未运行的原生/Android 平台写为通过。本版范围继续为 Yank 自有同步，微信输入法不在本版。
