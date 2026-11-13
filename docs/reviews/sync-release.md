# 自有同步发布阻塞复核

日期：2026-11-13。最终限定复核结论：**原五项 P1 及派生 Pair 撤权窗口均关闭；在这些已报告场景与修复路径内，没有发现仍需阻止产物构建的 P0/P1。** 这不是整个产品或所有部署环境的发布认证；产物构建、打包与真实设备验证由主 agent 继续。

本轮收到稳定信号后复核当前共享工作树，只写本报告，未改业务代码、测试、协议或生成代码，未提交、推送、发布。采用 `check`、`rust-best-practices`、`ddia-principles` 的审查约束。此前获授权实现过 sync 修复；本轮同时重点复核主 agent 的 core/network/worker 修复，避免仅验证自己负责的代码。首轮失败场景及旧快照留在文末，属于历史证据，其“暂不发布”和“剩余门槛”不再代表当前状态。

## 关闭依据

| 项目 | 当前结果 | 源码与反例证据 |
|---|---|---|
| P1-01 因果合并不稳定 | 已关闭 | `core/src/lib.rs:500` 的本地写入使用 Lamport `write_order` / `write_actor`，接收合并观察远端 Lamport；`:969` 的决胜键优先使用载荷写入次序。`save_local_record` 经 merge 保留并发分支，不把旧载荷简单覆盖为新观察状态。`replication.rs:347` 对原 A/B/C 因果后继删除反例运行全部 6 种到达顺序并重放，最终一致保留 C。 |
| P1-02 元数据编辑覆盖远端 formats | 已关闭 | `core/src/lib.rs:541` 在读取记录之前启动写事务；`:989` 为 SQLite Immediate，读取、闭包修改、合并和 outbox 更新同事务。`replication.rs:397` 用真实两连接、线程 barrier/通道检查阻塞与最终富格式可恢复，不是单连接顺序调用模拟并发。 |
| P1-03 probe/pull 间 epoch 重置跳历史 | 已关闭 | `client/src/sync.rs` 对 pull 响应 epoch 再校验；与预期不同的页不 apply，切换预期 epoch 后从 0 重拉。最多 3 次实际 pull，连续重置返回错误，不提交旧 receive cursor。`:619` 真实 mock TCP 在 probe 后重置并延迟零游标重拉，检查重拉前旧 cursor 未变，稳定页到达后富格式历史恢复；`:669` 连续重置反例检查有界退出。 |
| P1-04 撤销设备 trust 后旧任务/剪贴板仍生效 | 已关闭 | `network.rs:185` 获取 PeerPermit，`:204` 在共享授权锁下验证 trusted + generation 并执行本地操作。exchange 读取/构造页与 response apply 都走此锁，网络等待用 watch + biased select 取消；入站 Exchange 的响应发送也检查取消。剪贴板队列携带来源 permit，`worker.rs:516` 消费时同锁校验，再检查偏好、当前版本及未删除状态。queued exchange、inflight response、queued clipboard 三个反例均通过。 |
| P1-05 撤销存储权限后的迟到 ack/cursor 提交 | 已关闭 | `sync.rs:24` 的 authorized 在 prefs mutex 内校验 auth_revision、upload、URL、account_id、token，并执行本地 SQL；无网络 await 持锁。每次网络返回后重新校验，pending ack、snapshot sent cursor、receive page 都在 guard 内提交。延迟 ack 反例覆盖关闭 upload、同偏好 ABA、账号/token/URL 变化，并检查 pending、clip、sent/receive cursor 不被旧任务改变。 |
| 派生 Pair：已出队任务/慢设备码查询绕过撤权 | 已关闭 | `network.rs:114` 在 queue_pair 入队时捕获 peer generation；session 把三字段队列中的原 revision 传入 pair。`:677` 发送前检查原代次，trust_peer 在授予信任时再次持锁检查。UI 在 PairCode 入队时捕获 pair_epoch；revoke_peer 同步递增 epoch，worker 查询前检查、查询完成后 queue_pair(..., Some(epoch)) 在授权锁内再次检查。旧查询不能被当作新的用户确认。 |

协议检查：`proto/yank/v1/yank.proto:46` / `:47` 使用新增 tag 15/16 表达 write_order/write_actor；core wire 双向映射及生成的 prost 字段相符。本轮执行 `buf lint`，退出码 0；未运行生成或手改生成代码。当前尚未发布，本结论不覆盖将来与不携带这些因果标签的旧节点混用。

派生 Pair 的实际测试 `drained_pair_intent_cannot_regrant_trust_after_unpair` 从队列取出旧三元组后撤权，再调用 pair，检查失败、信任表为空、对端未接到连接；同一测试拒绝旧 code epoch，允许之后的新明确确认。UI 测试验证 Unpair 在 worker 执行前已撤权，并且后续 PairCode 捕获新 epoch。慢查询反例直接注入查询完成后的旧 epoch 到 queue_pair 边界；没有把它描述为真实慢 HTTP 设备码服务测试。response grant 的第二次检查另由源码复核确认。

设备信任与账号权限仍各自独立：登录、同账号发现和关联提示不自动授予 peer trust；账号退出/上传关闭通过 auth_revision 失效旧存储任务，显式设备撤权通过 PeerPermit 失效设备任务。Login cancel / old Save 的反例也已在稳定日志通过。服务器已接受的撤权前请求不会被客户端撤销，此处保证的是撤权后旧结果不能再修改本地状态。

存储恢复行为明确为：**server storage 页只恢复本地历史，不更新系统剪贴板；系统剪贴板仅由获授权的 peer live 更新。** 这是当前实现与测试确认的行为，历史不会因避免 receive_page 重入 prefs 锁而被丢弃。

重复页/ack、format/blob 与租户边界未发现独立于上述已修复场景的新增阻塞：operation fingerprint 拒绝同 ID 不同内容、ack 按 operation 精确删除 pending；页与 cursor 在本地事务应用。wire/page 展开完整格式数据；并发富格式恢复由 P1-02 反例覆盖。服务端认证身份选择用户 Store，密码变更与会话撤销同事务；本轮未重跑或扩展认证审计，首轮结论与稳定日志中的 11 个 server 测试继续作为此边界的证据。

## 验证结果与边界

已读取用户提供的完成日志 `/tmp/yank-release-candidate-tests.log`。用户说明执行命令为 `cargo test --locked --workspace`；本轮没有重跑全量 suite。日志终态为 **69 passed、0 failed、0 ignored**：client 20（含 8 sync）、interface 13、core unit 2、replication 20、ditto 3、server 11，其余目标为零测试。对应上表因果六顺序、真实两连接事务、延迟 TCP ack/reset、撤权取消、queued Pair 与 Login/Save 用例均显示 `ok`。早前 sync 定向修复记录为旧实现 5 fail / 3 pass，修复后 8/8；最终 suite 再次包含全部 8 项。该早前记录不替代本次读取的最终完成日志。

本轮新增执行的验证为只读源码/工作树检查、`buf lint` 和报告空白检查；没有复跑无关测试。测试日志不是所有未来文件变更的冻结证明，取证以下表源码内容为准。

未实测：真实 NAT 穿透/公网网络、真实 Ditto 客户端互操作、Windows/macOS/Android 跨 OS 设备与系统剪贴板行为。loopback 上的加密 relay-only 测试不等于真实 NAT 或真实 Ditto 验证；queued clipboard 测试也不等于真实 OS sink 集成。产物 build、安装/打包、签名与发布不在此次签核范围，仍由主 agent 执行。

共享工作树含大量他人修改，HEAD 不能代表候选全部内容。本轮最终取证 SHA-256：

| 文件 | SHA-256 |
|---|---|
| `crates/yank-core/src/lib.rs` | `ea60ad01e58c035c9cea7e17287bdb32a40e84916266b526e58985954c7af966` |
| `crates/yank-core/src/wire.rs` | `faebee79bdb68275ba083f6c4fdeabc5679591bdef102c53da125ab1c7106065` |
| `crates/yank-core/tests/replication.rs` | `79d56c3f58f8b0267db0b7bbe950a8d5f127b96610f8195ffd2663ba0d932a77` |
| `crates/yank-client/src/network.rs` | `b8b7d780fa732990d861df48c9dcfeea3ce364754a9234a4ec8e4598af66f4d8` |
| `crates/yank-client/src/sync.rs` | `36ff2b3f7b22193e314cec95d900bd685ef02448b4643c47a4247d22f309e65b` |
| `crates/yank-client/src/worker.rs` | `457de064a0e8f75158369d9b28fc78fe5f0cc4c8eef31f5877e3d0c71118108e` |
| `crates/yank-client/src/ui/mod.rs` | `ac3959e006f5b561a43c2c9952579fd443a82861f28be1d79243121985c4751f` |
| `crates/yank-server/src/auth.rs` | `e62cb06f9372dedccb69c3f4c39583bd53262ba096a5111f9023f8bb7098dc24` |
| `crates/yank-server/src/main.rs` | `b23a81369831e90cf79e184c8abf2047fb95d3297666f713285e571796b2d391` |
| `proto/yank/v1/yank.proto` | `62d7a3f0edab1c42d7522485d5fb3c42740ab8063644e24dba32f9eb14dee9b2` |
| `crates/yank-proto/src/generated/yank/v1/yank.v1.rs` | `e6c17fdcef2b3e0fda7f066695277ab2b072d0bd7044a824d7fe9eea1eab91d7` |
| `/tmp/yank-release-candidate-tests.log` | `249fb101e4700c45f402316522b6d07c4dbc3dbd396c62355e1f3589a5a5010c` |

## 最终完成记录

1. **已完成：** 原五项 P1 的实现路径及定向反例复核，全部关闭；原失败描述保留以便追溯。
2. **已完成：** 派生 queued Pair / 慢 code 查询窗口的入队、发送与 grant 代次检查；明确实际测试与静态检查的边界。
3. **已完成：** 读取最终 workspace 成功日志，核对 69 个通过结果；本轮 buf lint 通过，只更新本报告。
4. **不适用：** 本轮业务代码修改、提交、推送与发布。
5. **剩余：** 主 agent 的产物构建及真实环境验证；这些不改变本次限定范围内五项 P1 与 Pair 窗口的关闭状态。

---

## 首轮审查原文（历史证据，以下问题状态已由上文覆盖）

日期：2026-11-13。结论：**暂不发布；发现 5 项 P1，未发现有足够证据定为 P0 的问题。**

这是对当前共享工作树的独立只读审查，重点是新的 core/client，同步正确性优先；不是 Clips 能力矩阵审查，也不是修复交付。唯一产品仓库写入是本报告，没有修改实现、测试、协议或生成代码，没有提交、推送或发布。

审查范围：`yank-core/src/{lib,wire}.rs`、`yank-client/src/{network,sync,worker}.rs`，以及 `yank-server/src/{main,auth}.rs` 的账号、会话与存储隔离边界。阅读了 Rust/DDIA 技能并完成定向历史检索；历史检索没有提供能替代当前源码判断的 Yank 同步约束。

## P1-01：合并后的向量让已经过时的载荷重新成为胜者

**位置：** `crates/yank-core/src/lib.rs:532–565`，`resolution_key`（文件末尾）。

`merge` 用版本向量判断先后，但并发时按随机 UUID `version_id` 排序选载荷，再把两个版本的向量合并进胜者。这个合成状态声称观察过输家，却没有保留“当前载荷本身是哪次写入”的因果顺序。随机 UUID 排序不保证后继写入大于祖先，合并也不满足结合律。

**最小输入：** 从公共祖先 `{a:1}` 分叉；三个版本有同一个 clip ID，载荷分别为 A/B/C，`version_id` 与向量如下；使用合法 UUID，排序第一项不同，因此 canonical hash 不影响结果。

| 版本 | version_id | clock | 含义 |
|---|---|---|---|
| A | `f0000000-0000-4000-8000-000000000000` | `{a:2}` | A 分支编辑 |
| B | `80000000-0000-4000-8000-000000000000` | `{a:1,b:1}` | 与 A 并发的编辑 |
| C | `10000000-0000-4000-8000-000000000000` | `{a:3}` | A 的后继删除，`deleted=true` |

分别向两个空 `Store` 调用 `receive([A,B,C])` 与 `receive([A,C,B])`。前者留下 **A + `{a:3,b:1}`**，后者留下 **B + `{a:3,b:1}`**。再向前者重放 C，会因为合成向量支配 C 而直接忽略 C。两个结果互相交换后，也会按 UUID 选回过时的 A。这不是“并发编辑必须输给删除”的要求：B 与 C 的取舍可以有明确策略，但已经被 C 因果替代的 A 不应重新参与取胜。

**影响：** 同一批合法版本按不同到达顺序产生不同载荷；恢复出的旧内容能够携带删除的因果计数继续传播。冲突表保存副本不能修复主记录的因果错误。

**最小修复方向：** 保留现有单胜者模型时，增加不可变且满足因果先后顺序的写入排序标签（例如 Lamport 写入序号加 actor；本地新写入必须大于已观察到的标签），合并向量只表示已见上下文，不改写载荷标签。另一条正确路线是维护真正的非支配版本集合，再从集合选展示结果。仅更换随机 UUID、加墙钟、或多跑几轮 gossip 均不能修复因果语义。

**验证要求：** 固定上述 ID，枚举六种顺序并重放全部输入；断言相同最终状态且不以 A 为主记录。再覆盖有后继关系的删除/恢复及三副本传播。现有 `three_concurrent_replicas_converge_in_different_merge_orders` 创建的是三个互相并发的编辑，没有这个因果链。

**本次证据：** 对照源码逐分支运行的内存 Python 模型输出 `ABC/BAC -> A`、`ACB/BCA/CAB/CBA -> B`，全部最终向量为 `{a:3,b:1}`；不是新增 Rust 用例的实际执行结果。

## P1-02：本地元数据编辑可静默覆盖刚接收的远端内容及 formats

**位置：** `crates/yank-core/src/lib.rs:488–508,524–529`；调用方 `crates/yank-client/src/worker.rs:304–324`。

`mutate` 在事务外读取完整 clip 并调用修改闭包，随后才进入 `save_local` 的事务。`save_local_record` 会再次读取最新记录，但只合并它的 clock，不合并它的载荷。网络与 worker 使用独立连接，因此下面的顺序成立：

1. worker 执行 Pin/修改 collection，`mutate` 读到 X0（text/formats 为 F0），暂停在调用 `save_local` 前。
2. 网络接收 X1，携带新内容 F1 及 `{a:1,b:1}`，提交成功。
3. worker 继续保存旧 X0 加 pin/collection；`save_local_record` 读到 X1，却只取其向量，写回 **F0 + `{a:2,b:1}`**。
4. 再重放 X1，因为本地向量支配 X1，被直接忽略；X1 没有经过冲突分支，因此也没有为这次覆盖新增冲突恢复条目。

**影响：** 仅点击收藏或移动分组，就能丢掉并发到达的文字、HTML/RTF 或图片原始字节；错误载荷随后进入 outbox 并覆盖其他副本。这不属于用户主动编辑旧版本的冲突。

**最小修复：** 将 `get -> apply closure -> save_local_record -> outbox` 放到一个写事务里，事务从首次读取之前开始；使用合适的 SQLite 写锁/冲突重试，不嵌套调用当前自带事务的 `save_local`。另一种办法是版本 CAS，检测读取版本变更后重新对最新记录执行元数据闭包。不能只合并最新向量然后继续写旧载荷。

**最小复现方式：** 给 `mutate` 的首次 `get` 后加测试 barrier，另一连接执行 `receive(X1)` 后释放 barrier。无 barrier 的顺序替身也可用公开 API 表达：保留 `get(X0)` 返回值，接收 X1，再对保留值改 pin 并 `save_local`。断言最终格式为 F1 且 pin 已更新；本版会保留 F0。

**本次证据：** 源码与连接/事务边界推导；未改代码添加 barrier，未宣称已经运行该 Rust 失败用例。

## P1-03：探测与拉取之间发生 epoch 重置，会把旧高游标写到新 epoch 下

**位置：** `crates/yank-client/src/sync.rs:16–21,83–103`；`crates/yank-core/src/lib.rs:637–684`。

客户端先 probe epoch，再用该 epoch 找游标；实际拉取响应的 `page.epoch` 没有与 probe 比较。`Store::page` 在没有返回记录时保留请求的 `since`，`apply_page` 则直接存 `page.epoch:page.cursor`。

**确定的失败顺序：**

1. 本地记录 `server.cursor.<root>.<account> = E1:100`。
2. probe 成功返回 E1。
3. 服务端数据库重置/重建为 E2；由另一设备写入一条本地从未见过的远端记录 R，序号为 1。
4. 本轮实际请求仍为 `pull?since=100`。服务端返回 `{epoch:E2,cursor:100,clips:[],more:false}`。
5. 客户端把 **E2:100** 写入 metadata，下一轮 probe E2 后仍从 100 拉取，永久漏掉 R。

可令本地只有一条已经完成 snapshot 上传的记录，下一轮重传不会把新服务器序号推过 100，保证失败稳定。即便以后序号超过 100，R 的旧变更也不会自动补发。

**最小修复：** 实际响应 epoch 与 probe 不同就不得提交旧请求游标；在同一账号授权仍有效的前提下从 `since=0` 重新拉取，并按实际 epoch 管理 ready/cursor。更稳妥的协议是 pull 同时携带客户端已知 epoch，服务端在事务内发现不匹配就自行从零返回（peer exchange 已有类似参数）。不要仅对空页游标 clamp：那仍可能跳过新 epoch 的早期记录。

**验证要求：** 在测试服务端的 probe 与实际 pull 之间切换 Store，并先写入远端独有的 R；断言 R 被下载、游标未成为 E2:100。现有 server-reset 测试在整次 `upload()` 之前替换 Store，覆盖不到该竞态。

**本次证据：** 源码推导及内存游标模型：新 epoch 的 seq `[1,2,3]` 对 `since=100` 返回空集合，保存新 epoch 的 100 后持续跳过这些记录；未运行真实网络重置测试。

## P1-04：取消设备信任没有使已排队的发送任务和剪贴板应用失效

**位置：** `crates/yank-client/src/worker.rs:415–416,492–503`；`crates/yank-core/src/lib.rs:718–720`；`crates/yank-client/src/network.rs:402–419,538–573,74–104`。

`Unpair` 只删除 peers 行。发送任务是从较早读取的 peers 列表创建的，`exchange` 在构建和发送本地 Page **之前不重新检查信任**，仅在收到响应后检查；后置检查不能撤回已经发给对方的内容。设备撤权也不触发 `config_key` 改变，不会让当前网络 session 的 jobs 被整体终止。

**最小失败顺序：** A/B 已同步完旧历史，调度器已经读取包含 B 的 peers 列表/创建 B 的任务，但该任务还没开始执行；用户取消 B；B 仍信任 A。恢复旧任务执行，A 会把当前 `store.page(sent,20)` 发送给 B。可在取消后、旧任务读取 Page 前新增私密记录，保证这不是撤权前已经发出的内容。虽然 A 在收到响应后发现 B 不再受信任而不接收返回页，B 已拿到新记录。

另一个不需要网络延迟的残留路径：`receive_page` 已把 B 的 live clip 放进 `remote_clipboard`，worker 随后先处理 Unpair，再在同一次循环取出队列并写入系统剪贴板。队列只有 Clip，没有来源设备/账号及授权代次，无法在消费时拒绝已撤权来源。

**最小修复：** 给设备授权加持久或共享的 generation，任务捕获授权代次；取消信任时增加代次并取消对应任务。发送前、接收提交前及 OS 剪贴板消费前重新验证相同授权代次，检查与状态修改必须共享明确的串行化边界。剪贴板队列携带来源与代次。只在 `exchange` 返回后查询 peers，或者只清一次队列，均挡不住随后完成的旧任务。

**验证要求：** 使用 barrier 控制“任务创建后/发送前”取消信任，再创建本地记录，断言对端没有得到记录；另测队列产生后取消信任，断言 clipboard sink 不被调用。现有 direct-pairing 测试在取消后才发起新的请求，证明不了旧任务已失效。

**本次证据：** 源码调度路径及检查位置；未运行新增端点 barrier 测试。取消前已经发送的数据不在此阻塞项的要求之内。

## P1-05：关闭服务器存储后，尚未返回的 ack 仍会改变本地数据库

**位置：** `crates/yank-client/src/sync.rs:22–43,74–82,92–103`；`crates/yank-core/src/lib.rs:629–634`；`crates/yank-client/src/network.rs:183–184`。

pending push 只在发送前检查上传权限，`await` 返回后无条件 `store.acknowledge`。此方法不仅删除 outbox，还会 merge 服务端返回的 clip。关闭 upload 不改变网络 `config_key`，不会取消已在运行的存储任务。最后 pull 的权限检查不覆盖前面的 ack 副作用。

**最小失败顺序：** 服务端已接受当前 pending，并将它与服务端更新的 X1 合并；测试服务器暂不返回 ack。用户关闭服务器存储（`shared.prefs.upload=false`）。随后释放 ack，旧任务仍将 X1 导入本地并删除 outbox 项。客户端已经撤回对服务器同步的许可，却仍接收并传播该响应的状态；已经被服务端处理的 push 无法撤销，但本地合并仍应受当前授权控制。快照上传路径还会在最后一个 await 后继续写 sent cursor。

**最小修复：** 存储任务绑定 account/server/session 及本地上传授权 generation；在每次 await 后、处理 ack、推进 sent cursor 或接收 page 前验证它仍有效。检查与本地提交用同一个授权串行化机制，避免 bool 检查后又撤权的竞态。撤权后不应用 ack、不删除 pending；以后重新授权可依赖 operation ID 安全重试。账号切换及 session 撤销也应使代次失效。generation 可与 P1-04 采用同一机制，但设备信任与账号授权仍须分别管理。

**验证要求：** 在 mock push 返回前用 barrier 关闭 upload，释放 ack 后断言 local clip/outbox/cursor 均未被旧任务改变；另测同 URL 不同账号及旧 token。现有 opt-out 测试是在 `upload()` 开始前关闭权限，覆盖不到已经在 await 的任务。

**本次证据：** 源码路径；未执行新增延迟 ack 网络用例。该问题不声称服务器能撤回撤权前已经接受的请求。

## 其余重点边界与未升级为阻塞的项

- **设备 trust 与账号分离：** 当前读到的路径没有把登录、同账号发现或 account hint 自动转成 trust。Pair 要验证连接公钥与设备码，接收端需要明确同意；已配对 peer 表与账号会话分开。登录默认关闭 upload，logout 不删除独立建立的设备 trust，这本身符合分离语义。P1-04 针对的是显式取消 trust 后的旧任务，不要求退出账号自动取消所有设备。
- **服务端认证/租户存储：** 数据文件由已验证 User.id 派生，native/JSON push 都先认证并写该用户 Store；改密/重置与会话删除同事务。此次没有找到绕过账号身份选择其他用户存储文件的明确 P0/P1。公开 native 设备码查询属于配对发现，不等于已获存储或剪贴板访问权限，不能仅凭查询公开就判定跨租户数据泄漏。
- **重复 operation / old ack：** 服务端保存 fingerprint 与 ack，同 operation 不同载荷被拒绝；客户端按 operation 精确删除 pending，因此旧 ack 不会直接删除随后编辑产生的新 operation。这个已覆盖行为不等于 P1-05 的授权检查已覆盖。
- **重复页：** 相同 expanded clip 的重复 merge 会走相等分支；事务内合并与 cursor 一起提交。没有把这部分已有正向测试重新当作新验证。P1-01 的因果链和 P1-03 的 epoch 切换不在该保证之内。
- **formats/blob：** selected `get`、page、pending、冲突恢复和 export 会 expand；wire 携带完整 format 名称、可选 MIME 与字节。`list` 返回空 data 加内部 blob_refs 是有意的元数据列表路径，不能据此判为传输丢 blob。未发现独立于 P1-02 的正常 round-trip 格式字节丢失 P0/P1；本地元数据操作的竞争覆盖则确实会丢掉最新 formats。

## 证据范围和复核基线

已阅读 `/tmp/yank-full-tests.log`：该日志记录 50 个测试通过（client 6、interface 12、core 单元 2、replication 16、ditto 3、server 11，另有零测试目标）。**没有重新运行相同测试，也没有把日志当成当前所有共享工作树改动的完整验证。** 当前 replication 文件已包含日志外的新测试，说明日志不是工作树冻结证明。

本次新增执行仅为只读源码/状态检查及无文件输出的内存模型，没有运行新增 Rust 失败用例、真实网络 barrier 或 OS 剪贴板 sink。各项已分别说明证据等级；修复后的发布门槛是执行上述定向反例并取得实际结果，而不是只重跑旧全量测试。

Git HEAD 为 `21d2753fe420fbd36de62f7c2e04757718802825`，存在大量已跟踪与未跟踪的共享改动，结论以工作树内容为准。审查期间 client network 与 server main 的 LAN interface 过滤改动引起行号移动，已重新核对相关同步代码及引用。取证 SHA-256：

| 文件 | SHA-256 |
|---|---|
| `crates/yank-core/src/lib.rs` | `dbff9960ff8bf0991ccaac72eb6652d235f5242dda6c89aa8e2a561ed03aa238` |
| `crates/yank-core/src/wire.rs` | `dc1e85d77d258988c0824e0c3f8b4a4d431da74dbdf4ad3902670db63ee728d2` |
| `crates/yank-client/src/network.rs` | `009795ae7e485c907f527392524bcff7ef737affb41817a61a0b8184c3137ee0` |
| `crates/yank-client/src/sync.rs` | `0175e0fc696a51b25582f3e5d758b9659598da9771e4f07085ce1168c65af6e0` |
| `crates/yank-client/src/worker.rs` | `5d96e3d328ed9bea1bf666aa1469016368fcd8bf4b3ec1e7c8ae08786fe837b3` |
| `crates/yank-server/src/auth.rs` | `e62cb06f9372dedccb69c3f4c39583bd53262ba096a5111f9023f8bb7098dc24` |
| `crates/yank-server/src/main.rs` | `b23a81369831e90cf79e184c8abf2047fb95d3297666f713285e571796b2d391` |
| `/tmp/yank-full-tests.log` | `a777813be4e8ee65a3a31305fb973aeaa2a7dea99a7944eddde591b7f5098a5e` |

## 完成记录

1. **已完成：** 覆盖指定 core/client/server 边界，列出 5 项 P1 的位置、失败时序、最小修复和定向验证门槛。
2. **已完成：** 核对现有成功日志及覆盖缺口；将模型证据与实际 Rust/网络测试结果分开标注。
3. **不适用：** 本轮没有授权修复、提交、推送或发布。
4. **剩余发布门槛：** 实现修复并实际运行五类定向反例；当前报告不构成 release-ready 签字。

## 附注：本地文件路径许可限定复核（2026-11-13）

结论：**该有界修复未发现新增 P0/P1，可继续既定产物流程。** 本轮只读核查 core、clipboard、worker 的四处路径，仅追加本附注；未修改代码、重跑测试、提交或推送。定向历史检索未得到相关 Yank 约束，结论以当前源码为准。

1. **本地许可：** `core/src/lib.rs` 的 authorize_local_files 只对 FileList 写入本地 metadata `local.files.<clip.id> = content_hash`；can_copy_local_files 同时绑定 ID 与完整 formats 序列的 SHA-256，校验不依赖可伪造的 device_id。同 ID 路径/格式字节变更即不匹配。许可不属于 Clip/Page/wire 字段；export 的 Backup 仅含 version、clips、views，import 不授权。全量本地数据库复制不在“export 备份不转移许可”的保证内。
2. **授权来源：** NativeClipboard.capture 在真实 OS 采集后授权；worker 的 Command::Capture 仅 CaptureSource::System 授权。核对 UI 来源选择：桌面读取 OS files，mobile/foreground 不授予 System 来源；Ditto incoming、同步 receive、import 均未调用授权。核心 capture 本身不自动授权，避免把收到或导入的内容当作本机采集。
3. **消费边界：** worker Copy/Paste 对 FileList 校验本地许可；Copy plain 可绕过许可检查，但 NativeClipboard.write 的 plain 路径跳过整个格式循环，仅输出文本，不写 Files。文件格式识别与 write 的 file_list_paths 使用同一 is_file_list 条件，没有看到格式别名绕过这道检查。
4. **远端自动应用：** remoteClipboard 消费仍先拒绝任何 FileList，再写系统剪贴板，不因这次新增本地许可而自动放行远端文件路径。

已阅读 `/tmp/yank-file-cap-tests.log`：core 单元 2、replication 21 全部通过，含 remote_file_paths_cannot_gain_local_capability_by_forging_device_identity。该反例证明普通 capture 不授权、本机显式授权可用、伪造 device_id 且改路径后许可失效。**测试覆盖小缺口（非实现阻塞）：** backup 断言导出时该记录已因路径变更失去有效许可；建议另在许可仍有效时 export/import 并断言目标无许可，才能直接防回归“有效 cap 随备份转移”。当前不转移的判断另有上述 Backup 结构与授权调用点的静态依据。本轮未做真实跨 OS 文件剪贴板验证，也未把此前 69 项 workspace 日志当作本补丁的全量验证。

附注取证 SHA-256（本附注之后的修改需另核对）：

| 文件 | SHA-256 |
|---|---|
| `crates/yank-core/src/lib.rs` | `bc072777aade862f5ce84e2124569ffa018767315cadb08c67f1f720e8dae9c1` |
| `crates/yank-core/tests/replication.rs` | `3c5f03f6dc16589ee6b86971b71033aa619dc21080d75e97950e5dc45a550ef8` |
| `crates/yank-client/src/clipboard.rs` | `a81cc5691c2aab2816b141e8549c720297b530d7091fb1f6f32699803353b680` |
| `crates/yank-client/src/worker.rs` | `423bbdc3c38dcf3903a6750413a119aaa638c572112d2533e280b93134d12043` |
| `/tmp/yank-file-cap-tests.log` | `be5a7cd382f40a838f9e84ad2344a559c8f04430aa08c614f192a978ecc73286` |
