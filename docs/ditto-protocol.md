# Ditto 网络适配器

`yank-ditto` 是独立实现的 rutis 0.21.0 插件。默认关闭，仅监听 `127.0.0.1:23443`，启用需要非空共享密码。监听、连接任务和并发配额由插件持有，禁用或重新配置会释放监听端口。

协议依据是 Ditto 项目公开的 `CSendInfo` 定义及网络行为：
https://github.com/sabrogden/Ditto 。研究文件包含 `Shared/Defines.h`、网络发送/接收和加密实现；没有复制其实现代码、UI、资源或依赖。Ditto 是独立项目，其商标与授权不因本适配器而改变。

协议块使用传统 Ditto/KeePass 式 AES-256-CBC 包装：360 字节头部加密为 508 字节，140 字节加密包头、SHA-256 内容摘要和 100000 轮 AES 密钥变换。解析先检查长度、签名、轮数和摘要，限制单条大小、格式数量、连接数及超时。这是旧协议互通适配，不是 Yank 自有同步使用的加密协议。Yank 自有同步走 Iroh/QUIC 加密与 Protobuf。

支持 CF_UNICODETEXT（UTF-16LE）、HTML Format、Rich Text Format、PNG 和原始命名格式。可接收远端 Ditto 主动推送，也可把选中记录发到 Ditto。Ditto 没有这里可用的“拉取全部旧历史”接口，不能将“同步远端 Ditto”解释为自动扫描其数据库。

当前证据：加密校验测试、实际 TCP 文字/富文本推送测试、rutis 插件销毁释放端口测试。尚未在真实 Windows Ditto 进程上完成跨实现验收；DIB 图片转换、Ditto 群组/数据库导入和所有自定义格式的系统写回未实现。不要把本地回环测试当成完整 Ditto 兼容认证。
