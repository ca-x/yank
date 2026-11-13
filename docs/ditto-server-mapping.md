# 服务端与设备互通

旧未上线的单 token REST 同步设计已替换；无需维护旧数据库或旧 wire 兼容。

| 通道 | 认证与数据范围 |
| --- | --- |
| `/rpc/login`, `/rpc/logout` | Protobuf 二进制登录和当前会话撤销；logout 仅 bearer |
| `/rpc/health` | Protobuf ServerInfo；提供中继地址和存储能力 |
| `/rpc/register` | 设备公钥签名登记，90 秒时效；可访客登记 |
| `/rpc/devices` | 仅当前账号登记设备；不枚举访客 |
| `/rpc/devices/{code}` | 精确设备码查询；其他账号不能越权枚举 |
| `/rpc/sync/pull`, `/rpc/sync/push` | bearer、按账号数据库隔离，需服务端开启存储 |
| Iroh relay | 已登记端点获准连接，只转发加密设备流量 |
| mDNS | 发现 `_yank._udp.local.` 设备及 `_yank-server._tcp.local.` 实例；不等同配对 |
| `/api/*` | Web 管理 JSON，与设备同步协议分离；session cookie |

设备信任由本地首次确认建立，与账号是否登录无关。Ditto 不经过上述 RPC，使用单独可选插件；详见 [Ditto 协议](ditto-protocol.md)。数据和时间顺序规则详见 [设计契约](design/redesign.md)。
