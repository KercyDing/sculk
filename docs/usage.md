# sculk 使用说明（以当前源码为准）

本文覆盖 `sckc`（CLI）与 `sckt`（TUI）的完整使用方式。

## 1. 组件说明

- `sckc`：命令行客户端（建房、加入、relay 配置）。
- `sckt`：终端图形客户端（建房 / 加入 / 中继）。
- 默认端口：
  - Host（MC 服务端）`25565`
  - Join（本地入口）`30000`

## 2. CLI（`sckc`）

## 2.1 命令总览

```sh
sckc host [OPTIONS]
sckc join <TICKET> [OPTIONS]
sckc relay [--url <URL> | --list | --reset]
```

## 2.2 `sckc host`

作用：作为房主暴露本地 MC 服务端，输出可分享票据。

```sh
sckc host [OPTIONS]
```

可用参数：

| 参数 | 说明 | 默认值 |
| --- | --- | --- |
| `-p, --port <PORT>` | 本地 MC 服务端端口 | `25565` |
| `--new-key` | 强制生成新密钥（会改变身份和 ticket） | 关闭 |
| `--key-path <PATH>` | 指定密钥文件路径 | `{data_dir}/sculk/secret.key` |
| `-r, --relay <URL>` | 覆盖 relay 地址（优先级高于配置） | 无 |
| `-d, --delay <SECONDS>` | 路径状态打印间隔；`0` 表示仅变化时输出 | `0` |
| `--password <PWD>` | 连接密码 | 无 |
| `--max-players <N>` | 最大玩家数（按唯一 EndpointId） | 无上限 |

运行行为：
- 成功后输出：`Ticket: "sculk://..."`。
- 会尝试把带引号的 ticket 自动复制到剪贴板。
- 常驻运行，`Ctrl+C` 退出。

## 2.3 `sckc join`

作用：通过票据加入房主房间，在本地监听一个给 MC 客户端连接的端口。

```sh
sckc join "sculk://..." [OPTIONS]
```

可用参数：

| 参数 | 说明 | 默认值 |
| --- | --- | --- |
| `ticket` | 房主提供的票据（建议加引号） | 必填 |
| `-p, --port <PORT>` | 本地入口端口（MC 客户端连它） | `30000` |
| `-d, --delay <SECONDS>` | 路径状态打印间隔；`0` 表示仅变化时输出 | `0` |
| `--password <PWD>` | 连接密码 | 无 |
| `--max-retries <N>` | 最大重连次数；不传表示无限重连 | 无限 |

运行行为：
- 成功后提示：`Tunnel running. Connect MC client to 127.0.0.1:<port>`。
- 常驻运行，`Ctrl+C` 退出。

## 2.4 `sckc relay`

作用：管理持久化 relay 配置（写入 `profile.toml`）。

```sh
# 查看当前 relay 配置
sckc relay --list

# 设置自定义 relay
sckc relay --url https://your-relay.example.com

# 重置为默认 n0 relay
sckc relay --reset
```

说明：
- `--url` 会先校验 URL 合法性，成功后保存为自定义 relay。
- `--reset` 清除自定义，回退默认 n0 relay。
- 若 `relay` 子命令不带参数，会打印该子命令帮助。

## 2.5 CLI 事件输出

CLI 在运行时会输出隧道事件，例如：
- 玩家加入/离开
- 已连接/断开
- 路径切换（直连/中继 + RTT）
- 正在重连/重连成功
- 认证失败/玩家被拒绝
- 非致命错误

## 3. TUI（`sckt`）

## 3.1 启动

```sh
sckt
```

界面包含 3 个标签页：
- 建房
- 加入
- 中继

启动时会自动加载持久化配置：
- Host 端口
- Join 端口
- 上次 ticket
- relay 选择与 URL

## 3.2 主操作语义（Enter）

- 在「建房」页：
  - 空闲时 `Enter` 启动 Host 隧道
  - Host 活跃时 `Enter` 停止隧道
- 在「加入」页：
  - 空闲时 `Enter` 发起 Join
  - Join 活跃时 `Enter` 停止隧道
- 在「中继」页：
  - `Enter` 应用当前选中的 relay 配置（仅空闲态可切换）

## 3.3 按键总览

普通模式：
- `←/→`：切换标签（边界钳制，不循环）
- `Tab`：切换焦点（左侧配置 / 右侧日志）
- `↑/↓` 或 `k/j`：
  - 左侧：切字段（或中继条目）
  - 右侧：滚动日志
- `Enter`：执行当前标签主操作
- `i`：进入编辑模式
  - 在「中继」页只有选中“自建中继”时可进入编辑
- `c`：清空日志
- `h`：开关帮助弹窗

编辑模式：
- `Esc`：退出编辑并保存输入到 `profile.toml`
- `↑/↓`：切换字段
- `←/→`、`Home`、`End`：移动光标
- `Backspace`、`Delete`：删除字符
- 字符键：插入输入

帮助弹窗：
- `h` 或 `Esc` 关闭

停止确认弹窗（活跃态按 `Esc` 触发）：
- `y`：确认停止隧道
- `n` 或 `Esc`：取消

`Esc` 生命周期行为：
- `Starting`：取消启动
- `Active`：弹出停止确认
- `Stopping`：无额外动作
- `Idle`：1 秒内连按两次退出程序

## 3.4 TUI 运行时细节

- Host 启动成功后：
  - 票据写入状态并尝试复制剪贴板
  - host 密码输入框会清空
- Join 连通后：
  - join 密码输入框会清空
  - 当前 ticket 会保存到 `profile.join.last_ticket`
- 隧道关闭后：
  - host/join 密码都会清空
- 隧道活跃时会周期刷新连接快照（RTT、链路类型、连接数）

## 4. 票据（Ticket）格式

格式：
- 使用默认 relay：`sculk://<EndpointId>`
- 使用自定义 relay：`sculk://<EndpointId>?relay=<RelayUrl>`

注意：
- scheme 必须是 `sculk://`
- join 命令里建议给 ticket 加引号

## 5. 配置与数据目录

`sculk` 持久化目录：`{系统 data_dir}/sculk/`

常见系统路径：
- macOS：`~/Library/Application Support/sculk/`
- Linux：`~/.local/share/sculk/`
- Windows：`%APPDATA%\\sculk\\`

关键文件：
- `secret.key`：32 字节私钥（Host 身份）
- `profile.toml`：用户偏好（端口 / relay / last_ticket）

`profile.toml` 示例：

```toml
[host]
port = 25565

[join]
port = 30000
last_ticket = "sculk://..."

[relay]
custom = false
# url = "https://your-relay.example.com"
```

relay 最终生效优先级（高 -> 低）：
1. CLI `--relay <URL>` 显式传入
2. `profile.toml` 中 `relay.custom = true` 且存在 `relay.url`
3. 默认 n0 relay

## 6. 常用流程示例

## 6.1 房主（CLI）

```sh
sckc host --password 123456 --max-players 8
```

把输出的 `"sculk://..."` 票据发给玩家。

## 6.2 玩家（CLI）

```sh
sckc join "sculk://..." --password 123456
```

然后在 MC 客户端连接：`127.0.0.1:30000`。

## 6.3 切换自建 relay（CLI）

```sh
sckc relay --url https://your-relay.example.com
sckc relay --list
```

## 6.4 使用 TUI

```sh
sckt
```

- 左右切到目标标签
- `i` 编辑参数，`Esc` 保存
- `Enter` 启动/连接
- 右侧查看实时日志

