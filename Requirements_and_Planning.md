# icoding-client Requirements and Planning

## 1. 背景与目标

`icoding-client` 是一个桌面端 Agent 客户端。它运行在用户电脑上，登录云端账号后，通过一套 WebSocket 协议注册到云端智能体平台，并向被授权的云端智能体提供本机能力，例如浏览目录、读取文件、编辑文件、执行命令等。

核心目标：

- 让云端智能体可以在用户授权范围内操作本机资源。
- 提供简单、稳定、可长期驻留的桌面体验。
- 通过明确的协议、权限、审计和安全边界，降低远程操作本机带来的风险。
- 支持登录、注册、心跳、能力上报、任务执行、结果回传、错误恢复和版本升级。

非目标：

- 不在客户端内实现完整智能体推理能力，智能体主体在云端。
- 不默认提供无边界的本机控制能力，所有危险能力都应被权限、审计和策略约束。
- 不把客户端做成复杂 IDE；图形界面应以登录、状态、权限、日志和托盘操作为主。

## 2. 用户与使用场景

### 2.1 目标用户

- 普通用户：希望云端智能体帮助处理本机文件、运行命令、修改项目。
- 开发者：希望智能体接入本地开发环境，执行构建、测试、搜索、编辑代码。
- 企业/团队用户：希望统一管理客户端版本、设备注册、权限策略和审计记录。

### 2.2 典型流程

首次启动：

1. 用户双击启动客户端。
2. 客户端显示登录窗口。
3. 用户选择邮箱验证码或手机号验证码登录。
4. 登录成功后，客户端保存安全凭证。
5. 客户端采集必要系统信息，向云端注册当前设备。
6. 注册成功后建立 WebSocket 长连接。
7. 客户端提示连接状态，并设置开机自启。
8. 客户端进入托盘驻留模式。

日常使用：

1. 系统启动后客户端自启。
2. 客户端使用本地保存的 refresh token 或设备凭证恢复登录。
3. 客户端连接云端，发送认证、设备信息和能力清单。
4. 云端下发任务。
5. 客户端根据权限策略执行本机操作。
6. 客户端实时回传执行状态、输出、变更摘要和最终结果。

退出登录：

1. 用户从托盘选择退出登录。
2. 客户端断开 WebSocket。
3. 客户端清理本地登录凭证和设备会话。
4. 客户端回到登录窗口。

退出程序：

1. 用户从托盘选择退出程序。
2. 客户端向云端发送离线通知。
3. 客户端关闭 WebSocket、停止后台任务并退出进程。

## 3. 功能需求

### 3.1 登录与账号

支持两种登录方式：

- 邮箱 + 验证码。
- 手机号 + 验证码。

登录能力：

- 发送验证码。
- 验证验证码。
- 登录成功后获取 access token、refresh token、用户信息和租户/组织信息。
- 支持 token 自动刷新。
- 支持登录态过期后重新登录。
- 支持退出登录并清除本地凭证。

安全要求：

- token 不写入普通配置文件，统一保存在应用数据目录下的独立凭证文件中。
- Unix 系统将凭证文件权限限制为 `0600`，避免启动时触发系统凭证库鉴权。
- 本地只保存最小必要凭证。

macOS 启动权限：

- Agent 启动前检查“完整磁盘访问”，未授权时自动打开对应的系统设置页面。
- 权限未满足时不建立任务连接，避免任务执行途中再触发权限申请。
- macOS 不允许应用通过代码静默授予完整磁盘访问，必须由当前用户在系统设置中确认。

### 3.2 设备注册

客户端登录后应向云端注册当前设备，注册信息至少包含：

- 用户信息：
  - user_id。
  - organization_id/tenant_id，如果存在。
  - 登录方式。
- 设备信息：
  - device_id，本地生成并持久保存。
  - hostname。
  - OS 类型、版本、架构。
  - 当前用户名。
  - 客户端版本。
  - 客户端构建信息。
  - 时区、语言环境。
- 运行环境：
  - shell 类型。
  - 工作目录默认策略。
  - 可用能力列表。
  - 权限配置摘要。
- 网络信息：
  - 连接时间。
  - 客户端 IP 可由服务端识别，不建议客户端主动上报过多本地网络细节。

设备注册要求：

- 首次注册生成稳定 `device_id`。
- 同一用户多设备应可区分。
- 同一设备重新登录后应复用设备身份，但绑定当前登录用户。
- 云端可以禁用、下线或要求重新认证某个设备。

### 3.3 WebSocket 连接

客户端应通过 WebSocket 与云端保持长连接。

连接要求：

- 使用 `wss://`。
- 握手时携带 access token 或短期连接 token。
- 连接成功后发送 `client.hello` 或 `client.register` 消息。
- 支持心跳 ping/pong。
- 支持断线自动重连。
- 支持短暂断线后的会话恢复信息上报，包括上一 session、最近服务端消息 ID、运行中任务和待回传结果。
- 对断线期间正在执行的任务要有明确策略，避免云端任务长期停留在 running。
- 支持指数退避和最大重试间隔。
- 支持云端主动要求重新认证。
- 支持客户端版本过低时被云端拒绝连接。

状态要求：

- UI 和托盘菜单应能展示基本连接状态：
  - 未登录。
  - 正在连接。
  - 已连接。
  - 连接断开，正在重试。
  - 登录过期。
  - 设备被禁用。

### 3.4 本机能力

客户端第一阶段应提供以下能力。

文件系统：

- 浏览目录。
- 获取文件/目录元信息。
- 读取文本文件。
- 写入文件。
- 创建文件或目录。
- 删除文件或目录，通过路径策略后直接执行。
- 移动/重命名文件。
- 搜索文件名。
- 搜索文件内容。
- 返回文件 diff 或变更摘要。

命令执行：

- 执行 shell 命令。
- 支持工作目录。
- 支持环境变量白名单。
- 支持超时。
- 支持 stdout/stderr 流式回传。
- 支持取消正在运行的命令。
- 支持退出码回传。

系统信息：

- 获取客户端版本。
- 获取系统基础信息。
- 获取当前连接状态。
- 获取可用能力列表。

后续可选能力：

- 截图，需要明确用户授权。
- 应用窗口控制，需要更强权限模型。
- 浏览器自动化，需要单独权限、审计和可见提示。
- Git 操作封装。
- 文件变更监听。
- 本地端口服务探测。

### 3.5 图形界面

客户端需要一个简单图形化界面。

最小界面：

- 登录页：
  - 邮箱登录。
  - 手机号登录。
  - 验证码发送与倒计时。
  - 登录错误提示。
- 状态页：
  - 当前账号。
  - 当前设备名。
  - 云端连接状态。
  - 客户端版本。
  - 开机自启状态。
  - 最近任务/最近连接错误的简要记录。
- 权限页，建议第一版提供：
  - 允许访问的目录范围。
  - 是否允许执行命令。
- 日志页，建议第一版提供：
  - 最近连接日志。
  - 最近任务日志。
  - 错误日志。

界面原则：

- 首次启动显示主窗口。
- 登录后可关闭主窗口但保留托盘。
- 点击托盘可打开主窗口。
- 出错时应能从托盘或通知进入详情页。

### 3.6 托盘

托盘菜单至少包含：

- 打开主窗口。
- 连接状态展示。
- 当前登录账号展示。
- 退出登录。
- 退出程序。

建议后续增加：

- 暂停接收任务。
- 恢复接收任务。
- 最近任务。
- 打开日志目录。

托盘行为：

- 登录后默认驻留托盘。
- 主窗口关闭不等于退出程序。
- 退出程序需要显式操作。
- 退出登录后应断开云端连接并回到登录态。

### 3.7 开机自启

登录成功后默认开启开机自启，但应允许用户关闭。

平台实现：

- macOS: LaunchAgent 或系统登录项。
- Windows: Startup folder 或注册表 Run 项。
- Linux: XDG autostart desktop entry。

要求：

- 自启配置失败要提示用户。
- 自启状态应在 UI 中可见。
- 用户退出登录时可以保留或关闭自启，建议默认保留程序自启但处于未登录状态。
- 用户退出程序不应自动关闭自启。

### 3.8 更新与兼容

客户端应支持版本上报和兼容控制。

要求：

- 每次注册时上报客户端版本。
- 云端可返回最低支持版本。
- 版本过低时 UI 提示升级。
- 协议应包含 `protocol_version`。
- 能力列表应可扩展，避免服务端假设所有客户端都有同一组能力。

## 4. 安全与权限设计

这是项目最关键的部分。该客户端本质上把本机能力暴露给云端智能体，必须默认谨慎。

### 4.1 权限原则

- 默认最小权限。
- 用户可见。
- 命令与文件操作不进行逐次确认，由本地策略直接决定是否允许。
- 所有任务可审计。
- 云端策略和本地策略都要生效，本地策略是最后防线。
- 客户端不执行超出本地策略允许范围的任务，即使云端下发。

### 4.2 文件访问策略

建议第一版引入允许目录列表：

- 默认允许用户选择一个或多个工作目录。
- 只允许访问允许目录内的文件。
- 禁止通过 `..`、符号链接或路径规范化漏洞逃逸允许目录。
- 对隐藏目录、系统目录、凭证目录增加默认保护。
- 对大文件读取设置大小限制。
- 对二进制文件读取默认只返回元信息，除非明确请求 base64 且在大小限制内。

默认禁止的路径：

- 用户 SSH 密钥目录。
- 系统密钥链/凭证目录。
- 浏览器 profile 数据目录。
- 钱包、密码管理器和系统配置目录。
- 根目录、系统目录和应用程序目录。

### 4.3 命令执行策略

命令执行是高风险能力。

建议第一版策略：

- 默认允许命令执行，用户可通过单一开关整体关闭。
- 支持命令黑名单/高风险模式识别。
- 支持工作目录限制在允许目录内。
- 默认不注入敏感环境变量。
- 命令超时默认 5 分钟，可由任务指定但受本地上限限制。
- 单条命令输出大小需要限制，超限后截断并提示。

高风险命令示例：

- 删除大量文件。
- 修改系统配置。
- 修改权限。
- 下载并执行远程脚本。
- 访问密钥、浏览器数据或系统凭证。
- 关机、重启、格式化、磁盘分区等系统级操作。

### 4.4 执行许可模型

- 不对命令执行和删除操作进行逐次确认。
- 命令执行只保留“是否允许执行命令”开关，默认允许。
- 文件操作由允许目录、阻止目录和大小限制直接裁决。

### 4.5 审计日志

客户端本地应记录审计日志，云端也应记录。

本地审计内容：

- 任务 ID。
- 云端会话 ID。
- 操作类型。
- 操作参数摘要。
- 涉及路径。
- 开始时间和结束时间。
- 执行结果。
- 错误信息。

敏感信息处理：

- 日志中不记录 token。
- 命令输出可能包含秘密，需要支持脱敏或限制保存。
- 文件内容默认不完整写入本地日志，只记录摘要。

## 5. 协议初稿

### 5.1 基本约定

消息格式建议使用 JSON。

通用字段：

```json
{
  "id": "msg_...",
  "type": "client.register",
  "timestamp": "2026-01-01T00:00:00Z",
  "protocol_version": "1.0",
  "payload": {}
}
```

请求/响应约定：

- 云端下发请求时携带 `id`。
- 客户端响应使用 `reply_to` 指向原请求 ID。
- 长任务通过事件流回传进度。
- 错误使用统一结构。

错误结构：

```json
{
  "code": "PERMISSION_DENIED",
  "message": "Path is outside allowed directories",
  "details": {}
}
```

### 5.2 客户端注册

客户端连接后发送：

```json
{
  "type": "client.register",
  "payload": {
    "device_id": "dev_...",
    "client_version": "0.1.0",
    "protocol_version": "1.0",
    "user": {
      "user_id": "usr_...",
      "organization_id": "org_..."
    },
    "system": {
      "hostname": "MacBook-Pro",
      "platform": "macos",
      "os": "macos",
      "os_name": "macOS",
      "os_version": "15.5",
      "os_build": "24F74",
      "kernel_version": "24.5.0",
      "family": "unix",
      "arch": "arm64",
      "username": "alice",
      "timezone": "+08:00",
      "locale": "zh_CN.UTF-8",
      "shell": "/bin/zsh",
      "current_dir": "/Users/alice/Projects/demo",
      "executable_path": "/Applications/iCoding Client.app/Contents/MacOS/icoding-client"
    },
    "capabilities": [
      "fs.list",
      "fs.read",
      "fs.write",
      "process.exec",
      "process.cancel"
    ],
    "policy_summary": {
      "allowed_roots": ["~/Projects"],
      "shell_exec_enabled": true
    }
  }
}
```

云端响应：

```json
{
  "type": "server.registered",
  "reply_to": "msg_...",
  "payload": {
    "session_id": "sess_...",
    "server_time": "2026-01-01T00:00:01Z",
    "min_client_version": "0.1.0",
    "heartbeat_interval_seconds": 30
  }
}
```

### 5.3 心跳

客户端发送：

```json
{
  "type": "client.ping",
  "payload": {
    "session_id": "sess_..."
  }
}
```

云端响应：

```json
{
  "type": "server.pong",
  "reply_to": "msg_...",
  "payload": {
    "server_time": "2026-01-01T00:00:30Z"
  }
}
```

### 5.4 文件能力

目录列表：

```json
{
  "type": "task.request",
  "payload": {
    "task_id": "task_...",
    "capability": "fs.list",
    "params": {
      "path": "~/Projects/demo",
      "recursive": false,
      "limit": 200
    }
  }
}
```

读取文件：

```json
{
  "type": "task.request",
  "payload": {
    "task_id": "task_...",
    "capability": "fs.read",
    "params": {
      "path": "~/Projects/demo/src/main.rs",
      "encoding": "utf-8",
      "max_bytes": 200000
    }
  }
}
```

写入文件：

```json
{
  "type": "task.request",
  "payload": {
    "task_id": "task_...",
    "capability": "fs.write",
    "params": {
      "path": "~/Projects/demo/src/main.rs",
      "mode": "overwrite",
      "content": "fn main() {}",
      "encoding": "utf-8",
      "expected_sha256": "optional_previous_hash"
    }
  }
}
```

写入要求：

- 支持 `expected_sha256` 防止并发覆盖。
- 返回写入前后 hash。
- 对覆盖、删除、跨目录移动等操作应用权限策略。

### 5.5 命令执行

启动命令：

```json
{
  "type": "task.request",
  "payload": {
    "task_id": "task_...",
    "capability": "process.exec",
    "params": {
      "command": "cargo test",
      "cwd": "~/Projects/demo",
      "timeout_seconds": 300,
      "env": {}
    }
  }
}
```

流式输出：

```json
{
  "type": "task.event",
  "payload": {
    "task_id": "task_...",
    "event": "process.output",
    "stream": "stdout",
    "chunk": "running 3 tests\n"
  }
}
```

结束事件：

```json
{
  "type": "task.result",
  "payload": {
    "task_id": "task_...",
    "status": "completed",
    "exit_code": 0,
    "duration_ms": 1234
  }
}
```

取消命令：

```json
{
  "type": "task.cancel",
  "payload": {
    "task_id": "task_..."
  }
}
```

### 5.6 任务生命周期

任务状态：

- `queued`
- `running`
- `completed`
- `failed`
- `cancelled`
- `rejected`
- `timed_out`

客户端应能处理：

- 同时多个低风险文件任务。
- 命令执行并发限制，建议第一版最多 1 个。
- 云端取消任务。
- 客户端本地策略拒绝任务。
- 网络断开后的任务恢复或失败上报。

## 6. 技术架构建议

当前项目是 Rust `edition = 2024` 的最小工程。建议保持 Rust 作为核心实现语言。

### 6.1 推荐模块划分

```text
src/
  main.rs
  app/
    mod.rs
    state.rs
    config.rs
  auth/
    mod.rs
    api.rs
    token_store.rs
  device/
    mod.rs
    fingerprint.rs
    autostart.rs
  ws/
    mod.rs
    client.rs
    protocol.rs
    reconnect.rs
  capabilities/
    mod.rs
    fs.rs
    process.rs
    system.rs
  policy/
    mod.rs
    path_policy.rs
    command_policy.rs
  ui/
    mod.rs
    tray.rs
    window.rs
  audit/
    mod.rs
    log.rs
```

### 6.2 桌面框架选择

候选方案：

- Tauri:
  - 优点：Rust 生态契合、体积较小、跨平台、托盘和自启支持较成熟。
  - 缺点：需要前端技术栈。
- egui/eframe:
  - 优点：纯 Rust、简单、适合工具型 UI。
  - 缺点：系统集成、托盘、自启、WebView 能力可能需要额外处理。
- 原生平台 UI:
  - 优点：系统体验最好。
  - 缺点：跨平台成本最高。

建议：

- 如果目标是跨平台桌面客户端，优先考虑 Tauri。
- 如果第一版只追求快速验证协议和本机能力，先做 Rust 后台核心 + 简单 UI，再逐步补齐 Tauri 壳。

### 6.3 异步运行时

建议使用：

- `tokio` 作为异步运行时。
- `tokio-tungstenite` 或 Tauri 兼容的 WebSocket 客户端。
- `serde`/`serde_json` 定义协议结构。
- `tracing` 做结构化日志。

### 6.4 本地数据

本地需要保存：

- 设备 ID。
- 用户偏好。
- 权限策略。
- 自启状态。
- 最近日志索引。

建议位置：

- macOS: `~/Library/Application Support/icoding-client`
- Windows: `%APPDATA%/icoding-client`
- Linux: `$XDG_CONFIG_HOME/icoding-client`

敏感数据：

- token 放入应用数据目录下仅当前用户可读写的独立凭证文件。
- 普通配置文件只保存非敏感数据。

## 7. 云端 API 需求

除 WebSocket 外，云端还需要 HTTP API。

认证：

- `POST /auth/email/code/send`
- `POST /auth/email/code/verify`
- `POST /auth/mobile/code/send`
- `POST /auth/mobile/code/verify`
- `POST /auth/token/refresh`
- `POST /auth/logout`

设备：

- `POST /devices/register`
- `POST /devices/heartbeat`
- `POST /devices/logout`
- `GET /devices/current/policy`

WebSocket：

- `GET /agent/client/ws`

可选：

- `GET /client/releases/latest`
- `POST /logs/client`
- `POST /audit/events`

## 8. 配置项

建议支持以下配置：

```toml
[server]
api_base_url = "https://apilite.icoding.ink"
ws_url = "wss://apilite.icoding.ink/api/v1/agent/ws"

[client]
auto_start = true
start_minimized = true
log_level = "info"

[policy]
allowed_roots = []
shell_exec_enabled = true
max_file_read_bytes = 1048576
max_command_output_bytes = 10485760
default_command_timeout_seconds = 300
```

生产环境里服务器地址建议由构建配置或受保护配置控制，避免普通用户误连到伪造服务。

## 9. 错误处理与恢复

需要覆盖：

- 网络不可用。
- DNS/TLS 失败。
- token 过期。
- refresh token 失效。
- 设备被云端禁用。
- 协议版本不兼容。
- 权限不足。
- 文件不存在。
- 文件编码不支持。
- 文件过大。
- 命令超时。
- 用户拒绝授权。
- 客户端升级要求。

恢复策略：

- 网络错误自动重连。
- token 过期自动刷新。
- refresh 失败回到登录页。
- 设备禁用时停止所有远程任务。
- 协议不兼容时提示升级。
- 任务级错误只影响当前任务，不应导致客户端崩溃。

## 10. 可观测性

日志：

- 使用结构化日志。
- 区分 app、auth、ws、task、policy、capability。
- 支持滚动日志。
- UI 提供最近日志。

指标：

- WebSocket 连接次数。
- 重连次数。
- 心跳延迟。
- 任务成功/失败数量。
- 权限拒绝数量。
- 命令平均耗时。

调试：

- 开发版可开启 verbose 日志。
- 生产版避免输出敏感信息。

## 11. 测试计划

单元测试：

- 协议序列化/反序列化。
- 路径规范化与越权检测。
- 文件读写边界。
- 命令策略判断。
- token 存储抽象。
- 重连退避算法。

集成测试：

- 模拟 WebSocket 服务端。
- 登录和 token refresh 流程。
- 设备注册流程。
- 文件任务执行。
- 命令任务执行。
- 任务取消。

端到端测试：

- 首次登录。
- 登录后托盘驻留。
- 开机自启配置。
- 云端下发任务并执行。
- 退出登录。
- 断网后重连。

安全测试：

- 路径穿越。
- 符号链接逃逸。
- 大文件读取。
- 命令注入。
- 日志脱敏。
- token 泄漏扫描。

## 12. 分阶段计划

### Phase 0: 需求确认与协议定稿

目标：

- 明确云端 API 形态。
- 明确 WebSocket 消息结构。
- 明确第一版权限策略。
- 选择桌面框架。

产出：

- 协议文档。
- 权限策略文档。
- UI 草图。
- 技术选型结论。

### Phase 1: 后台核心 MVP

目标：

- 实现配置加载。
- 实现设备 ID。
- 实现登录 API 调用占位或真实对接。
- 实现 WebSocket 连接、注册、心跳、重连。
- 实现基础文件能力。
- 实现命令执行能力，并提供默认开启的本地总开关。

产出：

- 可连接云端的后台客户端。
- 可执行模拟任务。
- 基础日志。

### Phase 2: 桌面 UI 与托盘

目标：

- 实现登录窗口。
- 实现状态窗口。
- 实现托盘菜单。
- 实现退出登录/退出程序。
- 实现开机自启。

产出：

- 用户可双击使用的桌面应用。
- 登录后托盘驻留。

### Phase 3: 权限与审计强化

目标：

- 允许目录配置。
- 本地允许目录与命令总开关。
- 审计日志。
- 命令策略。
- 日志脱敏。

产出：

- 安全边界可用的第一版客户端。
- 可排查的本地审计记录。

### Phase 4: 稳定性与发布

目标：

- 自动更新或升级提示。
- 崩溃恢复。
- 更完善的错误提示。
- 跨平台打包。
- 端到端测试。

产出：

- 可小范围发布的 Beta 版本。

### Phase 5: 高级能力

目标：

- 文件变更监听。
- Git 能力封装。
- 截图和窗口能力。
- 浏览器自动化能力。
- 企业策略下发。

产出：

- 更完整的云端智能体本机执行环境。

## 13. 关键待确认问题

- 云端服务的正式域名、API 路径和认证协议是什么？
- 是否需要支持多租户/组织切换？
- 客户端第一版目标平台是 macOS only，还是 macOS/Windows/Linux 同时支持？
- 命令执行默认是否开启？
- 文件访问默认允许哪些目录？
- 云端智能体任务是否需要多个会话并发？
- 是否需要企业管理员远程禁用客户端或下发策略？
- 是否需要自动更新，还是先提供手动下载升级？
- 是否要求离线模式，还是离线时只保持本地 UI 可用？

## 14. 建议的第一版验收标准

第一版可以定义为：

- 用户可以通过邮箱验证码或手机号验证码登录。
- 登录后客户端生成并注册设备。
- 客户端建立稳定 WebSocket 连接并保持心跳。
- 云端可以看到客户端在线、系统信息和能力列表。
- 云端可以下发目录浏览和文件读取任务。
- 云端可以下发文件写入任务，客户端按允许目录策略执行。
- 云端可以下发命令执行任务，客户端按启用开关和超时策略执行。
- 客户端可以流式回传命令输出。
- 用户可以从托盘退出登录或退出程序。
- 登录后开机自启生效。
- 本地有基础日志和审计记录。
- 路径越权、token 泄漏和危险命令有基础防护。

## 15. 初步技术依赖建议

如果采用 Rust + Tauri，可能需要：

- `tauri`: 桌面应用、窗口、托盘、系统集成。
- `tokio`: 异步运行时。
- `serde`, `serde_json`: 协议结构。
- `reqwest`: HTTP API。
- `tokio-tungstenite`: WebSocket。
- `tracing`, `tracing-subscriber`: 日志。
- `directories`: 跨平台配置目录。
- `uuid`: device_id 和 message_id。
- `sha2`: 文件 hash。
- `sysinfo`: 系统信息采集。

依赖选择需要在确定桌面框架后最终确认。
