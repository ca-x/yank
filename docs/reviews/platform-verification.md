# 发布前平台验证快照

2026-11-13。源码初次提交 7973ed4，CI runner 更新 ae1667f；尚未发布 v0.2.0，也未删除旧 tag/release。

- 本机 fmt、Buf lint/生成一致性、Clippy -D warnings、70项 workspace tests曾通过；修复后的无 DISPLAY 全量 74 项测试、Clippy -D warnings 也已通过。本快照在正式发布前生成，最终发布结果以 GitHub Release 和对应 Actions 为准。
- optimized Linux client/server构建通过，隔离 `/tmp/yank-native-review`，关闭自动采集/备份/更新。X100使用GSK_RENDERER=cairo，手动映射应用自己的WM frame。1000x640完整窗口与Android截图用于验证；不包含真实用户剪贴板。
- Android35 x86_64模拟器支持arm64翻译，APK arm64-v8a测试签名，安装启动通过。ADB reverse仅对本测试实例把17219/17220映射到本机回环服务器，不开放公网。
- 真实UI：Android输入Linux设备码、发起仅设备互通；Linux弹窗显示Android码并确认；双方未登录，upload=false。Linux持久peer数变为1。测试记录“Yank Android relay smoke 2026-11-13”出现在Android历史，页脚“UI review · 加密中继”。这验证实际原生跨平台纯中继的一向传输；独立 reviewer 随后通过真实触摸保存 Android 草稿，并以只读 SQLite 查询确认相同记录 ID 的追加正文到达 Linux；双向同步见 ui-ux-final.md，不据此宣称全部平台验收。
- 网络状态曾未触发界面更新：现Snapshot包含status/identity/discovery/server，并排序；新无DB变更的状态刷新测试通过。本机设备码文本订阅遗漏也已补。
- mDNS CPU取样：此主机73网卡，原daemon在2秒窗口约100%单核；排除容器veth/docker/lzc-br等之后另一次窗口约1.5%。保留普通桥/VPN；不是全面性能基准。
- CI 43379896990：macos-14 runner在执行前不受支持，更新到macos-15；旧run取消。
- CI 43380435339：Linux phone模拟捕获被宿主无DISPLAY隐私检查误拦；现通过CaptureSource区分desktop system与foreground，桌面检查仍保留，去掉DISPLAY的定向测试通过。macOS Ditto roundtrip超时：旧AES ARM默认软件后端且同步KDF阻塞async；现加密移到spawn_blocking，并仅在macOS/Linux ARM开启aes_armv8运行时检测，测试等待与45秒插件预算一致。Windows测试未完成前为读取失败日志取消整run，不宣称Windows测试通过。
- 另修周期任务按入站事件重置sleep造成tick停顿/重复调度：改为单调Instant deadline，加入入站频率无关测试。

未验证：真实ARM手机、Windows/macOS GUI、实际公网NAT、真实Windows Ditto跨实现互通。Linux deb需Ubuntu CI实际依赖探测/构建，不以本机Arch二进制冒充Ubuntu发行包。
