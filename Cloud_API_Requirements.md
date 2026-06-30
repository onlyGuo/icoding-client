# Cloud API Requirements

本文档描述 `icoding-client` 需要云端提供的 HTTP API。登录相关接口已经由后端确定，本文会原样记录；其他接口为客户端侧建议需求，用于后续后端实现和前后端联调。

## 1. 通用约定

### 1.1 Base URL

示例：

```text
https://apilite.icoding.ink
```

生产环境、测试环境和本地开发环境应使用不同配置。

### 1.2 API 前缀

现有登录接口使用：

```text
/api/v1
```

建议新增接口也保持该前缀。

### 1.3 认证方式

推荐：

```http
Authorization: Bearer <token>
```

兼容方式：

```http
token: <token>
access_token: <token>
access-token: <token>
```

兼容 query：

```http
?token=<token>
?access_token=<token>
?access-token=<token>
```

客户端默认只使用 `Authorization: Bearer <token>`。

### 1.4 响应格式建议

现有接口可能直接返回业务对象或空响应体。新增接口建议统一使用业务对象直接返回，错误由 HTTP 状态码和错误 JSON 表达。

错误响应建议：

```json
{
  "code": "DEVICE_DISABLED",
  "message": "Device has been disabled",
  "details": {}
}
```

常见状态码：

- `200`: 成功。
- `204`: 成功且无响应体。
- `400`: 请求参数错误。
- `401`: 未登录或 token 无效。
- `403`: 无权限、设备禁用、策略拒绝。
- `404`: 资源不存在。
- `409`: 状态冲突。
- `426`: 客户端版本过低，需要升级。
- `429`: 请求过于频繁。
- `500`: 服务端错误。

### 1.5 时间格式

统一使用 ISO 8601 UTC：

```text
2026-06-25T12:00:00Z
```

### 1.6 ID 建议

- 用户 ID 沿用后端现有数字 ID。
- 设备 ID 由客户端生成并持久保存，建议格式为 `dev_` 前缀 UUID/ULID。
- WebSocket session ID 由服务端生成。
- 任务 ID 由服务端生成。
- 消息 ID 可由发送方生成。

## 2. 现有登录接口

### 2.1 初始化/获取当前用户

```http
GET /api/v1/user
```

作用：

- 获取当前登录用户信息。
- 未登录返回 `null`。
- 同时会在 session 里设置 `validation=true`，后续发送验证码需要它。

请求头可选：

```http
Authorization: Bearer <token>
```

响应示例：

```json
{
  "id": 1,
  "email": "test@example.com",
  "mobile": null,
  "nicker": "tes****",
  "avatar": null
}
```

客户端要求：

- 启动登录页时先调用一次该接口，用于初始化 session。
- 若本地已有 token，则携带 token 调用该接口确认当前登录态。
- 若返回 `null`，客户端视为未登录。

### 2.2 发送验证码

```http
POST /api/v1/user/sendVerificationCode
Content-Type: application/json
```

邮箱验证码：

```json
{
  "type": "email",
  "email": "test@example.com"
}
```

手机号验证码：

```json
{
  "type": "mobile",
  "mobile": "13800138000"
}
```

说明：

- 验证码有效期 5 分钟。
- 正常情况下需要先调用 `GET /api/v1/user` 初始化 session。
- 如果 `type` 以 `MM` 开头，可以跳过 session validation。

跳过 session validation 示例：

```json
{
  "type": "MMemail",
  "email": "test@example.com"
}
```

成功时无响应体。

客户端要求：

- 普通登录流程默认先调用 `GET /api/v1/user`，再发送验证码。
- 如果桌面客户端无法可靠维护后端 session，可使用 `MMemail`/`MMmobile`，但应由后端确认这是允许的客户端场景。
- 发送成功后前端进入倒计时，建议 60 秒后允许重发。

### 2.3 验证验证码并登录/注册

```http
POST /api/v1/user/verify
Content-Type: application/json
```

邮箱登录：

```json
{
  "type": "email",
  "email": "test@example.com",
  "code": "123456"
}
```

手机号登录：

```json
{
  "type": "mobile",
  "mobile": "13800138000",
  "code": "123456"
}
```

行为：

- 验证码正确：登录成功。
- 用户不存在：自动注册新用户。
- 返回 token，有效期 7 天。
- 后续每次携带 token，后端会自动续期 7 天。

响应示例：

```json
{
  "token": "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  "user": {
    "id": 1,
    "email": "test@example.com",
    "mobile": null,
    "nicker": "tes****"
  }
}
```

客户端要求：

- 登录成功后将 token 写入应用数据目录下的独立文件，并限制为仅当前用户可读写。
- user 信息可写入普通配置缓存，但需要以服务端返回为准。
- 登录成功后立即执行设备注册。

### 2.4 登出

后端目前没有专门 logout 接口。

客户端行为：

- 退出登录时直接删除本地 token。
- 断开 WebSocket。
- 清理当前会话状态。
- 回到登录页。

后续建议：

```http
POST /api/v1/user/logout
Authorization: Bearer <token>
```

该接口不是第一版必需，但未来可用于服务端主动失效 token 或记录审计事件。

## 3. 设备接口

设备接口用于让云端知道哪些桌面客户端在线、属于哪个用户、具备哪些能力、运行在什么系统环境中。

### 3.1 注册/更新设备

```http
POST /api/v1/agent/devices/register
Authorization: Bearer <token>
Content-Type: application/json
```

请求：

```json
{
  "device_id": "dev_01HZ...",
  "device_name": "Alice-MacBook-Pro",
  "client_version": "0.1.0",
  "protocol_version": "1.0",
  "user": {
    "id": 1,
    "email": "test@example.com",
    "mobile": null,
    "nicker": "tes****",
    "avatar": null
  },
  "system": {
    "hostname": "Alice-MacBook-Pro",
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
    "fs.stat",
    "fs.read",
    "fs.write",
    "fs.mkdir",
    "fs.move",
    "fs.delete",
    "fs.search",
    "process.exec",
    "process.cancel",
    "system.info"
  ],
  "policy_summary": {
    "allowed_roots": [
      "/Users/alice/Projects"
    ],
    "shell_exec_enabled": true
  }
}
```

响应：

```json
{
  "device_id": "dev_01HZ...",
  "server_device_id": 123,
  "status": "enabled",
  "display_name": "Alice-MacBook-Pro",
  "ws_url": "wss://apilite.icoding.ink/api/v1/agent/ws",
  "connection_token": "short_lived_ws_token_optional",
  "connection_token_expires_at": "2026-06-25T12:10:00Z",
  "server_time": "2026-06-25T12:00:00Z",
  "min_client_version": "0.1.0",
  "latest_client_version": "0.1.0",
  "policy": {
    "max_command_timeout_seconds": 300,
    "max_file_read_bytes": 1048576,
    "max_command_output_bytes": 10485760,
    "allow_shell_exec": true
  }
}
```

说明：

- 如果不想引入独立 `connection_token`，WebSocket 可以直接使用登录 token。
- `connection_token` 更安全，建议为短期 token，只用于 WebSocket 建连。
- 服务端可以在响应里下发策略上限，本地策略和服务端策略取更严格者。

### 3.2 获取当前设备状态

```http
GET /api/v1/agent/devices/{device_id}
Authorization: Bearer <token>
```

响应：

```json
{
  "device_id": "dev_01HZ...",
  "server_device_id": 123,
  "status": "enabled",
  "last_seen_at": "2026-06-25T12:00:00Z",
  "disabled_reason": null,
  "policy": {
    "allow_shell_exec": true
  }
}
```

用途：

- 客户端启动时确认设备是否仍可用。
- UI 展示设备状态。

### 3.3 更新设备状态

```http
PATCH /api/v1/agent/devices/{device_id}
Authorization: Bearer <token>
Content-Type: application/json
```

请求：

```json
{
  "device_name": "Alice Work Laptop",
  "client_version": "0.1.0",
  "capabilities": [
    "fs.list",
    "fs.read",
    "process.exec"
  ],
  "policy_summary": {
    "allowed_roots": [
      "/Users/alice/Projects"
    ],
    "shell_exec_enabled": true
  }
}
```

响应：

```json
{
  "device_id": "dev_01HZ...",
  "status": "enabled",
  "updated_at": "2026-06-25T12:00:00Z"
}
```

用途：

- 用户修改本地策略后同步云端。
- 客户端升级后同步新版本和能力列表。

### 3.4 设备离线通知

```http
POST /api/v1/agent/devices/{device_id}/offline
Authorization: Bearer <token>
Content-Type: application/json
```

请求：

```json
{
  "reason": "user_quit",
  "active_session_id": "sess_01HZ..."
}
```

响应：

```json
{
  "ok": true
}
```

说明：

- 用户退出程序时调用。
- 网络异常或进程崩溃时无法保证调用，服务端仍需依赖 WebSocket 断开和心跳超时判断离线。

## 4. 策略接口

策略接口用于云端向客户端下发组织级或用户级约束。客户端本地策略仍是最后防线。

### 4.1 获取客户端策略

```http
GET /api/v1/agent/policy?device_id=dev_01HZ...
Authorization: Bearer <token>
```

响应：

```json
{
  "version": 3,
  "updated_at": "2026-06-25T12:00:00Z",
  "device_status": "enabled",
  "limits": {
    "max_file_read_bytes": 1048576,
    "max_file_write_bytes": 1048576,
    "max_command_timeout_seconds": 300,
    "max_command_output_bytes": 10485760,
    "max_concurrent_tasks": 4,
    "max_concurrent_processes": 1
  },
  "permissions": {
    "allow_fs_read": true,
    "allow_fs_write": true,
    "allow_fs_delete": false,
    "allow_shell_exec": true,
    "allow_screenshot": false,
    "allow_browser_control": false
  },
  "blocked_paths": [
    "~/.ssh",
    "~/.gnupg",
    "~/Library/Keychains"
  ],
  "blocked_command_patterns": [
    "rm -rf /",
    "curl * | sh",
    "shutdown",
    "reboot"
  ]
}
```

客户端行为：

- 每次设备注册后拉取策略。
- WebSocket 收到 `server.policy_updated` 时重新拉取。
- 本地配置和服务端策略冲突时，取更严格者。

## 5. WebSocket 入口接口

### 5.1 建立 WebSocket

```http
GET /api/v1/agent/ws
Authorization: Bearer <token>
```

或：

```http
GET /api/v1/agent/ws?connection_token=<short_lived_ws_token>
```

建议请求头：

```http
Authorization: Bearer <token>
X-Device-Id: dev_01HZ...
X-Client-Version: 0.1.0
X-Protocol-Version: 1.0
```

说明：

- 具体消息协议见 `WebSocket_Protocol.md`。
- 如果 HTTP 框架不方便读取 WebSocket Authorization header，可使用短期 `connection_token` query。

## 6. 审计与日志接口

第一版可以只做本地日志。若云端需要统一审计，建议提供以下接口。

### 6.1 上报审计事件

```http
POST /api/v1/agent/audit/events
Authorization: Bearer <token>
Content-Type: application/json
```

请求：

```json
{
  "device_id": "dev_01HZ...",
  "session_id": "sess_01HZ...",
  "events": [
    {
      "event_id": "evt_01HZ...",
      "task_id": "task_01HZ...",
      "type": "fs.read",
      "status": "completed",
      "started_at": "2026-06-25T12:00:00Z",
      "finished_at": "2026-06-25T12:00:01Z",
      "summary": {
        "path": "/Users/alice/Projects/demo/src/main.rs",
        "bytes": 1200
      }
    }
  ]
}
```

响应：

```json
{
  "accepted": 1
}
```

要求：

- 不上报 token。
- 默认不上报完整文件内容。
- 命令输出可能包含敏感信息，默认只上报摘要或截断内容。

### 6.2 上报客户端日志

```http
POST /api/v1/agent/logs
Authorization: Bearer <token>
Content-Type: application/json
```

请求：

```json
{
  "device_id": "dev_01HZ...",
  "session_id": "sess_01HZ...",
  "level": "error",
  "logs": [
    {
      "timestamp": "2026-06-25T12:00:00Z",
      "target": "ws",
      "message": "websocket disconnected",
      "fields": {
        "reason": "network_error"
      }
    }
  ]
}
```

响应：

```json
{
  "accepted": 1
}
```

说明：

- 该接口不是 MVP 必需。
- 更建议第一版先保留本地日志，遇到用户反馈时手动导出。

## 7. 版本与更新接口

### 7.1 查询最新客户端版本

```http
GET /api/v1/agent/client/releases/latest?platform=macos&arch=arm64&current_version=0.1.0
Authorization: Bearer <token>
```

响应：

```json
{
  "latest_version": "0.2.0",
  "min_supported_version": "0.1.0",
  "update_required": false,
  "download_url": "https://example.com/downloads/icoding-client-0.2.0-aarch64.dmg",
  "sha256": "...",
  "release_notes": "Bug fixes and stability improvements."
}
```

客户端行为：

- 如果 `update_required=true`，停止接收新任务并提示用户升级。
- 如果只是普通更新，状态页提示即可。

## 8. 任务查询接口

任务主要通过 WebSocket 下发和回传，不建议第一版再做复杂 HTTP 任务接口。

可选只读接口：

```http
GET /api/v1/agent/tasks/{task_id}
Authorization: Bearer <token>
```

响应：

```json
{
  "task_id": "task_01HZ...",
  "device_id": "dev_01HZ...",
  "status": "completed",
  "created_at": "2026-06-25T12:00:00Z",
  "started_at": "2026-06-25T12:00:01Z",
  "finished_at": "2026-06-25T12:00:03Z"
}
```

用途：

- UI 里展示最近任务详情。
- 调试任务状态。

## 9. 最小后端接口清单

MVP 必需：

- `GET /api/v1/user`
- `POST /api/v1/user/sendVerificationCode`
- `POST /api/v1/user/verify`
- `POST /api/v1/agent/devices/register`
- `GET /api/v1/agent/ws`

强烈建议：

- `GET /api/v1/agent/policy`
- `PATCH /api/v1/agent/devices/{device_id}`
- `POST /api/v1/agent/devices/{device_id}/offline`

可后置：

- `POST /api/v1/user/logout`
- `POST /api/v1/agent/audit/events`
- `POST /api/v1/agent/logs`
- `GET /api/v1/agent/client/releases/latest`
- `GET /api/v1/agent/tasks/{task_id}`

## 10. 客户端启动时 API 调用顺序

已有 token：

1. `GET /api/v1/user`
2. `POST /api/v1/agent/devices/register`
3. `GET /api/v1/agent/policy`
4. 建立 WebSocket：`GET /api/v1/agent/ws`
5. WebSocket 内发送 `client.register`

无 token：

1. `GET /api/v1/user`
2. `POST /api/v1/user/sendVerificationCode`
3. `POST /api/v1/user/verify`
4. 保存 token
5. `POST /api/v1/agent/devices/register`
6. `GET /api/v1/agent/policy`
7. 建立 WebSocket
8. WebSocket 内发送 `client.register`

退出登录：

1. 发送 WebSocket `client.goodbye`
2. 断开 WebSocket
3. 删除本地 token
4. 清理 UI 会话状态

退出程序：

1. 发送 WebSocket `client.goodbye`
2. 调用 `POST /api/v1/agent/devices/{device_id}/offline`
3. 断开 WebSocket
4. 退出进程
