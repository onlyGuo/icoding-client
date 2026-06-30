# WebSocket Protocol

本文档定义 `icoding-client` 与云端智能体平台之间的 WebSocket 消息协议。HTTP 接口见 `Cloud_API_Requirements.md`。

## 1. 目标

WebSocket 连接用于：

- 客户端向云端注册在线状态和本机能力。
- 云端下发任务。
- 客户端回传任务状态、流式输出和结果。
- 双方维持心跳。
- 云端下发策略更新、下线、升级等控制消息。

## 2. 连接入口

```http
GET /api/v1/agent/ws
Authorization: Bearer <token>
```

建议请求头：

```http
X-Device-Id: dev_01HZ...
X-Client-Version: 0.1.0
X-Protocol-Version: 1.0
```

如果服务端不方便在 WebSocket 握手中读取 Authorization header，也可以使用设备注册接口返回的短期连接 token：

```http
GET /api/v1/agent/ws?connection_token=<short_lived_ws_token>
```

要求：

- 必须使用 `wss://`。
- 服务端必须校验 token 和 device_id 的绑定关系。
- 连接建立后，客户端必须先发送 `client.register`。

## 3. 通用消息结构

所有消息均为 JSON 文本帧。

```json
{
  "id": "msg_01HZ...",
  "type": "client.register",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {}
}
```

字段说明：

- `id`: 消息 ID。由发送方生成。
- `type`: 消息类型。
- `timestamp`: 发送时间，ISO 8601 UTC。
- `protocol_version`: 协议版本。
- `payload`: 消息内容。

响应消息增加：

```json
{
  "reply_to": "msg_01HZ..."
}
```

错误消息统一结构：

```json
{
  "id": "msg_01HZ...",
  "type": "error",
  "reply_to": "msg_01HZ...",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "code": "PERMISSION_DENIED",
    "message": "Path is outside allowed roots",
    "details": {}
  }
}
```

## 4. 协议版本

初始版本：

```text
1.0
```

兼容规则：

- patch/minor 扩展应尽量向后兼容。
- 新增字段必须允许旧客户端忽略。
- 服务端不得向客户端下发其未声明的 capability。
- 客户端发现不支持的消息类型，应回复 `UNSUPPORTED_MESSAGE_TYPE`。
- 客户端发现不支持的 capability，应回复 `UNSUPPORTED_CAPABILITY`。

## 5. 连接生命周期

### 5.1 客户端注册

客户端连接建立后发送：

```json
{
  "id": "msg_01HZ_CLIENT_REGISTER",
  "type": "client.register",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "device_id": "dev_01HZ...",
    "client_version": "0.1.0",
    "user": {
      "id": 1,
      "email": "test@example.com",
      "mobile": null,
      "nicker": "tes****"
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
}
```

服务端响应：

```json
{
  "id": "msg_01HZ_REGISTERED",
  "type": "server.registered",
  "reply_to": "msg_01HZ_CLIENT_REGISTER",
  "timestamp": "2026-06-25T12:00:01Z",
  "protocol_version": "1.0",
  "payload": {
    "session_id": "sess_01HZ...",
    "server_time": "2026-06-25T12:00:01Z",
    "heartbeat_interval_seconds": 30,
    "max_missed_heartbeats": 3,
    "device_status": "enabled",
    "policy_version": 3
  }
}
```

如果设备被禁用：

```json
{
  "id": "msg_01HZ_DEVICE_DISABLED",
  "type": "error",
  "reply_to": "msg_01HZ_CLIENT_REGISTER",
  "timestamp": "2026-06-25T12:00:01Z",
  "protocol_version": "1.0",
  "payload": {
    "code": "DEVICE_DISABLED",
    "message": "Device has been disabled",
    "details": {
      "reason": "Disabled by administrator"
    }
  }
}
```

### 5.2 心跳

客户端按服务端返回的间隔发送：

```json
{
  "id": "msg_01HZ_PING",
  "type": "client.ping",
  "timestamp": "2026-06-25T12:00:30Z",
  "protocol_version": "1.0",
  "payload": {
    "session_id": "sess_01HZ..."
  }
}
```

服务端响应：

```json
{
  "id": "msg_01HZ_PONG",
  "type": "server.pong",
  "reply_to": "msg_01HZ_PING",
  "timestamp": "2026-06-25T12:00:30Z",
  "protocol_version": "1.0",
  "payload": {
    "server_time": "2026-06-25T12:00:30Z"
  }
}
```

要求：

- 客户端连续错过 `max_missed_heartbeats` 次 pong 后应断开并重连。
- 重连使用指数退避，建议 1s、2s、5s、10s、30s、60s，最大 60s。

### 5.3 短暂断线与重连

客户端必须支持短暂断线后的自动重连。这里的“短暂断线”指网络切换、电脑睡眠唤醒、代理变化、服务端滚动重启等导致的 WebSocket 临时中断。

第一版重连目标：

- 不要求无损续传所有流式输出。
- 要求客户端能自动恢复在线状态。
- 要求服务端能知道这是同一设备的新连接。
- 要求未开始执行的任务可以重新下发。
- 要求正在执行的命令任务在断线后有明确状态，不允许服务端长期停留在 running。

客户端断线后行为：

1. 标记连接状态为 `reconnecting`。
2. 暂停接收新任务。
3. 对正在执行的任务应用本地策略：
   - 文件读写类短任务：允许继续完成，但结果等重连后上报；如果结果缓存失败，则标记失败。
   - 命令执行类长任务：第一版建议继续运行一小段宽限期，例如 30 秒；如果宽限期内重连成功，继续回传；否则终止命令并标记 `connection_lost`。
4. 使用指数退避重连。
5. 重连成功后重新发送 `client.register`，并携带上一次 `session_id`。

重连注册示例：

```json
{
  "id": "msg_01HZ_RECONNECT",
  "type": "client.register",
  "timestamp": "2026-06-25T12:01:00Z",
  "protocol_version": "1.0",
  "payload": {
    "device_id": "dev_01HZ...",
    "client_version": "0.1.0",
    "previous_session_id": "sess_01HZ_OLD",
    "reconnect": true,
    "last_server_message_id": "msg_01HZ_LAST_SERVER",
    "running_tasks": [
      {
        "task_id": "task_01HZ...",
        "status": "running",
        "capability": "process.exec",
        "can_continue": true
      }
    ],
    "completed_pending_results": [
      "task_01HZ_DONE"
    ],
    "capabilities": [
      "fs.list",
      "fs.read",
      "process.exec"
    ]
  }
}
```

服务端响应：

```json
{
  "id": "msg_01HZ_RECONNECTED",
  "type": "server.registered",
  "reply_to": "msg_01HZ_RECONNECT",
  "timestamp": "2026-06-25T12:01:01Z",
  "protocol_version": "1.0",
  "payload": {
    "session_id": "sess_01HZ_NEW",
    "resumed_from_session_id": "sess_01HZ_OLD",
    "server_time": "2026-06-25T12:01:01Z",
    "heartbeat_interval_seconds": 30,
    "max_missed_heartbeats": 3,
    "resume": {
      "accepted": true,
      "tasks_to_continue": [
        "task_01HZ..."
      ],
      "tasks_to_cancel": [],
      "tasks_to_report": [
        "task_01HZ_DONE"
      ]
    }
  }
}
```

如果服务端不接受恢复：

```json
{
  "id": "msg_01HZ_RECONNECTED",
  "type": "server.registered",
  "reply_to": "msg_01HZ_RECONNECT",
  "timestamp": "2026-06-25T12:01:01Z",
  "protocol_version": "1.0",
  "payload": {
    "session_id": "sess_01HZ_NEW",
    "resumed_from_session_id": null,
    "resume": {
      "accepted": false,
      "reason": "previous_session_expired",
      "tasks_to_continue": [],
      "tasks_to_cancel": [
        "task_01HZ..."
      ],
      "tasks_to_report": []
    }
  }
}
```

客户端收到 `resume.accepted=false` 时：

- 停止本地仍在运行的远程任务。
- 将相关任务标记为 `failed` 或 `cancelled`。
- 等待服务端重新下发需要重试的任务。

### 5.4 消息确认与断线补偿

为了降低短暂断线造成的状态丢失，建议协议支持轻量 ACK。

客户端处理完服务端关键消息后发送：

```json
{
  "id": "msg_01HZ_ACK",
  "type": "client.ack",
  "timestamp": "2026-06-25T12:00:05Z",
  "protocol_version": "1.0",
  "payload": {
    "message_id": "msg_01HZ_TASK_REQUEST",
    "task_id": "task_01HZ...",
    "status": "received"
  }
}
```

需要 ACK 的服务端消息：

- `task.request`
- `task.cancel`
- `server.disconnect`
- `server.policy_updated`
- `server.upgrade_required`

服务端行为建议：

- 维护每个设备最近 N 条关键消息。
- 如果连接断开前没有收到 ACK，可以在重连后重新发送。
- `task.request` 必须通过 `task_id` 做幂等；客户端如果再次收到相同 `task_id`，不得重复执行有副作用的操作。

客户端幂等要求：

- 每个 `task_id` 只能进入一次实际执行。
- 对 `fs.write`、`fs.delete`、`fs.move` 这类有副作用任务，重连后重复收到同一任务时，应返回已有结果或 `TASK_ALREADY_HANDLED`，而不是重复执行。
- 对 `process.exec`，重复任务默认不重新启动进程，除非服务端明确下发新的 `task_id`。

### 5.5 客户端主动离线

```json
{
  "id": "msg_01HZ_GOODBYE",
  "type": "client.goodbye",
  "timestamp": "2026-06-25T12:10:00Z",
  "protocol_version": "1.0",
  "payload": {
    "session_id": "sess_01HZ...",
    "reason": "user_quit"
  }
}
```

reason 可选：

- `user_quit`
- `logout`
- `restart`
- `update`
- `network_switch`

### 5.6 服务端控制消息

要求客户端重新认证：

```json
{
  "id": "msg_01HZ_REAUTH",
  "type": "server.reauth_required",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "reason": "token_expired"
  }
}
```

要求客户端下线：

```json
{
  "id": "msg_01HZ_SHUTDOWN",
  "type": "server.disconnect",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "reason": "device_disabled",
    "message": "Device has been disabled"
  }
}
```

策略已更新：

```json
{
  "id": "msg_01HZ_POLICY",
  "type": "server.policy_updated",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "policy_version": 4
  }
}
```

客户端收到后应调用：

```http
GET /api/v1/agent/policy?device_id=dev_01HZ...
```

版本过低：

```json
{
  "id": "msg_01HZ_UPGRADE",
  "type": "server.upgrade_required",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "min_client_version": "0.2.0",
    "download_url": "https://example.com/downloads/icoding-client.dmg"
  }
}
```

## 6. 任务模型

云端通过 `task.request` 下发任务，客户端通过 `task.accepted`、`task.event`、`task.result`、`task.failed` 回传状态。

任务状态：

- `received`
- `accepted`
- `running`
- `completed`
- `failed`
- `cancelled`
- `rejected`
- `timed_out`

### 6.1 下发任务

```json
{
  "id": "msg_01HZ_TASK_REQUEST",
  "type": "task.request",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "task_id": "task_01HZ...",
    "conversation_id": "conv_01HZ...",
    "agent_id": "agent_01HZ...",
    "capability": "fs.read",
    "params": {},
    "timeout_seconds": 300,
    "require_result": true
  }
}
```

客户端收到后必须先进行本地校验：

- capability 是否支持。
- 设备当前是否允许接收任务。
- 服务端策略是否允许。
- 本地策略是否允许。
- 参数是否合法。

### 6.2 接受任务

```json
{
  "id": "msg_01HZ_TASK_ACCEPTED",
  "type": "task.accepted",
  "reply_to": "msg_01HZ_TASK_REQUEST",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "task_id": "task_01HZ...",
    "status": "accepted"
  }
}
```

### 6.3 拒绝任务

```json
{
  "id": "msg_01HZ_TASK_REJECTED",
  "type": "task.rejected",
  "reply_to": "msg_01HZ_TASK_REQUEST",
  "timestamp": "2026-06-25T12:00:00Z",
  "protocol_version": "1.0",
  "payload": {
    "task_id": "task_01HZ...",
    "status": "rejected",
    "error": {
      "code": "INVALID_ARGUMENT",
      "message": "invalid task.request payload: missing field `path`",
      "details": {
        "causes": ["missing field `path`"]
      }
    }
  }
}
```

### 6.4 任务事件

```json
{
  "id": "msg_01HZ_TASK_EVENT",
  "type": "task.event",
  "timestamp": "2026-06-25T12:00:01Z",
  "protocol_version": "1.0",
  "payload": {
    "task_id": "task_01HZ...",
    "event": "progress",
    "status": "running",
    "message": "Reading file"
  }
}
```

### 6.5 任务完成

```json
{
  "id": "msg_01HZ_TASK_RESULT",
  "type": "task.result",
  "timestamp": "2026-06-25T12:00:02Z",
  "protocol_version": "1.0",
  "payload": {
    "task_id": "task_01HZ...",
    "status": "completed",
    "duration_ms": 1234,
    "result": {}
  }
}
```

### 6.6 任务失败

```json
{
  "id": "msg_01HZ_TASK_FAILED",
  "type": "task.failed",
  "timestamp": "2026-06-25T12:00:02Z",
  "protocol_version": "1.0",
  "payload": {
    "task_id": "task_01HZ...",
    "status": "failed",
    "duration_ms": 1234,
    "error": {
      "code": "NOT_FOUND",
      "message": "fs.read failed: failed to canonicalize /Users/alice/Projects/demo/missing.txt: No such file or directory (os error 2)",
      "details": {
        "capability": "fs.read",
        "causes": [
          "failed to canonicalize /Users/alice/Projects/demo/missing.txt",
          "No such file or directory (os error 2)"
        ]
      }
    }
  }
}
```

失败响应必须保留完整错误链，不能只返回 `fs.list failed` 之类的操作名：

- `message` 包含能力名、失败动作、相关路径以及底层系统或服务端原因。
- `details.capability` 标明失败的能力，`details.causes` 按顺序列出底层原因。
- `code` 使用可判定的分类，例如 `NOT_FOUND`、`PERMISSION_DENIED`、`POLICY_DENIED`、`INVALID_ARGUMENT`、`TIMEOUT`、`LIMIT_EXCEEDED`、`CONFLICT` 或 `NETWORK_ERROR`。

### 6.7 取消任务

服务端发送：

```json
{
  "id": "msg_01HZ_TASK_CANCEL",
  "type": "task.cancel",
  "timestamp": "2026-06-25T12:00:05Z",
  "protocol_version": "1.0",
  "payload": {
    "task_id": "task_01HZ...",
    "reason": "user_cancelled"
  }
}
```

客户端响应：

```json
{
  "id": "msg_01HZ_TASK_CANCELLED",
  "type": "task.cancelled",
  "reply_to": "msg_01HZ_TASK_CANCEL",
  "timestamp": "2026-06-25T12:00:05Z",
  "protocol_version": "1.0",
  "payload": {
    "task_id": "task_01HZ...",
    "status": "cancelled"
  }
}
```

## 7. Capability: system.info

请求：

```json
{
  "type": "task.request",
  "payload": {
    "task_id": "task_01HZ...",
    "capability": "system.info",
    "params": {}
  }
}
```

结果：

```json
{
  "type": "task.result",
  "payload": {
    "task_id": "task_01HZ...",
    "status": "completed",
    "result": {
      "client_version": "0.1.0",
      "protocol_version": "1.0",
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
        "fs.read",
        "process.exec"
      ]
    }
  }
}
```

## 8. Capability: fs.stat

请求 params：

```json
{
  "path": "/Users/alice/Projects/demo/src/main.rs"
}
```

结果：

```json
{
  "path": "/Users/alice/Projects/demo/src/main.rs",
  "kind": "file",
  "size": 1024,
  "modified_at": "2026-06-25T12:00:00Z",
  "readonly": false,
  "sha256": "optional_for_file"
}
```

kind：

- `file`
- `directory`
- `symlink`
- `other`

## 9. Capability: fs.list

请求 params：

```json
{
  "path": "/Users/alice/Projects/demo",
  "recursive": false,
  "include_hidden": false,
  "limit": 200
}
```

结果：

```json
{
  "path": "/Users/alice/Projects/demo",
  "entries": [
    {
      "name": "src",
      "path": "/Users/alice/Projects/demo/src",
      "kind": "directory",
      "size": null,
      "modified_at": "2026-06-25T12:00:00Z"
    },
    {
      "name": "Cargo.toml",
      "path": "/Users/alice/Projects/demo/Cargo.toml",
      "kind": "file",
      "size": 512,
      "modified_at": "2026-06-25T12:00:00Z"
    }
  ],
  "truncated": false
}
```

要求：

- `path` 必须落在允许目录内。
- 返回数量超过 `limit` 时设置 `truncated=true`。

## 10. Capability: fs.read

请求 params：

```json
{
  "path": "/Users/alice/Projects/demo/src/main.rs",
  "encoding": "utf-8",
  "max_bytes": 200000
}
```

结果：

```json
{
  "path": "/Users/alice/Projects/demo/src/main.rs",
  "encoding": "utf-8",
  "content": "fn main() {\n    println!(\"hello\");\n}\n",
  "size": 43,
  "sha256": "..."
}
```

二进制文件建议结果：

```json
{
  "path": "/Users/alice/Projects/demo/image.png",
  "encoding": "binary",
  "content_base64": "iVBORw0KGgo...",
  "size": 12345,
  "sha256": "..."
}
```

要求：

- 默认只读取文本文件。
- 二进制读取需要显式 `encoding=binary`。
- 文件大小超过策略上限时返回 `FILE_TOO_LARGE`。

## 11. Capability: fs.write

请求 params：

```json
{
  "path": "/Users/alice/Projects/demo/src/main.rs",
  "mode": "overwrite",
  "encoding": "utf-8",
  "content": "fn main() {}\n",
  "expected_sha256": "previous_hash_optional",
  "create_parent_dirs": false
}
```

mode：

- `create_new`: 只在文件不存在时创建。
- `overwrite`: 覆盖写入。
- `append`: 追加写入。

结果：

```json
{
  "path": "/Users/alice/Projects/demo/src/main.rs",
  "mode": "overwrite",
  "bytes_written": 13,
  "previous_sha256": "previous_hash_optional",
  "sha256": "new_hash"
}
```

要求：

- 覆盖已有文件时，如果提供了 `expected_sha256`，必须匹配才允许写入。
- `expected_sha256` 不匹配返回 `FILE_CHANGED`。
- 写入应尽量使用临时文件 + 原子替换。

## 12. Capability: fs.mkdir

请求 params：

```json
{
  "path": "/Users/alice/Projects/demo/new-folder",
  "recursive": true
}
```

结果：

```json
{
  "path": "/Users/alice/Projects/demo/new-folder",
  "created": true
}
```

## 13. Capability: fs.move

请求 params：

```json
{
  "from": "/Users/alice/Projects/demo/a.txt",
  "to": "/Users/alice/Projects/demo/b.txt",
  "overwrite": false
}
```

结果：

```json
{
  "from": "/Users/alice/Projects/demo/a.txt",
  "to": "/Users/alice/Projects/demo/b.txt",
  "moved": true
}
```

要求：

- `from` 和 `to` 都必须通过路径策略。
- `from` 和 `to` 均在允许目录内时直接执行，不进行逐次确认。

## 14. Capability: fs.delete

请求 params：

```json
{
  "path": "/Users/alice/Projects/demo/old.txt",
  "recursive": false
}
```

结果：

```json
{
  "path": "/Users/alice/Projects/demo/old.txt",
  "deleted": true
}
```

要求：

- 路径通过本地允许目录与阻止目录策略后直接执行，不进行逐次确认。
- 目录递归删除同样受路径策略约束。
- 不允许删除允许根目录本身。

## 15. Capability: fs.search

请求 params：

```json
{
  "root": "/Users/alice/Projects/demo",
  "query": "main",
  "mode": "filename",
  "include_hidden": false,
  "limit": 100
}
```

mode：

- `filename`
- `content`

结果：

```json
{
  "root": "/Users/alice/Projects/demo",
  "matches": [
    {
      "path": "/Users/alice/Projects/demo/src/main.rs",
      "kind": "file",
      "line": 1,
      "preview": "fn main() {"
    }
  ],
  "truncated": false
}
```

要求：

- 内容搜索应限制文件大小。
- 可优先使用本机高效工具或 Rust 实现。

## 16. Capability: process.exec

请求 params：

```json
{
  "command": "cargo test",
  "cwd": "/Users/alice/Projects/demo",
  "timeout_seconds": 300,
  "env": {},
  "shell": "default"
}
```

客户端事件：

```json
{
  "type": "task.event",
  "payload": {
    "task_id": "task_01HZ...",
    "event": "process.started",
    "status": "running",
    "pid": 12345
  }
}
```

stdout/stderr 流式输出：

```json
{
  "type": "task.event",
  "payload": {
    "task_id": "task_01HZ...",
    "event": "process.output",
    "stream": "stdout",
    "chunk": "running 3 tests\n",
    "sequence": 1
  }
}
```

结果：

```json
{
  "type": "task.result",
  "payload": {
    "task_id": "task_01HZ...",
    "status": "completed",
    "duration_ms": 1234,
    "result": {
      "exit_code": 0,
      "stdout_truncated": false,
      "stderr_truncated": false
    }
  }
}
```

要求：

- `cwd` 必须在允许目录内。
- 是否允许命令执行仅由本地 `shell_exec_enabled` 控制，默认允许。
- 输出需要按 chunk 分片回传。
- 输出总量超过限制后截断。
- 命令超时后终止进程并返回 `COMMAND_TIMED_OUT`。

## 17. Capability: process.cancel

通常使用 `task.cancel` 即可。也可作为 capability 调用：

请求 params：

```json
{
  "task_id": "task_01HZ...",
  "signal": "terminate"
}
```

结果：

```json
{
  "task_id": "task_01HZ...",
  "cancelled": true
}
```

signal：

- `terminate`
- `kill`

客户端应先尝试温和终止，超时后再强制结束。

## 18. 执行许可模型

- 客户端不对命令执行、文件删除等操作进行逐次确认。
- 命令执行只由本地 `shell_exec_enabled` 开关控制，默认值为 `true`。
- 文件操作继续受允许目录、阻止目录、路径规范化和大小限制约束。

## 19. 并发与顺序

建议第一版：

- 文件读取/列表任务最多并发 4 个。
- 文件写入/移动/删除任务串行执行。
- 命令执行最多并发 1 个。
- 同一 `task_id` 的 `task.event` 按 sequence 或发送顺序处理。
- WebSocket 断线期间正在执行的任务按“短暂断线与重连”策略处理。

后续可增强：

- 支持更完整的断线续传，包括补发断线期间的全部流式输出。
- 服务端支持按 `last_server_message_id` 补发遗漏消息。

## 20. 错误码

通用：

- `BAD_REQUEST`
- `UNAUTHORIZED`
- `FORBIDDEN`
- `DEVICE_DISABLED`
- `PROTOCOL_VERSION_UNSUPPORTED`
- `UNSUPPORTED_MESSAGE_TYPE`
- `UNSUPPORTED_CAPABILITY`
- `TASK_NOT_FOUND`
- `TASK_ALREADY_HANDLED`
- `TASK_CANCELLED`
- `TASK_TIMED_OUT`
- `CONNECTION_LOST`
- `INTERNAL_ERROR`

文件：

- `PATH_OUTSIDE_ALLOWED_ROOTS`
- `PATH_BLOCKED`
- `FILE_NOT_FOUND`
- `FILE_ALREADY_EXISTS`
- `FILE_CHANGED`
- `FILE_TOO_LARGE`
- `READ_FAILED`
- `WRITE_FAILED`
- `DELETE_FAILED`
- `INVALID_ENCODING`

命令：

- `SHELL_EXEC_DISABLED`
- `COMMAND_REJECTED`
- `COMMAND_TIMED_OUT`
- `COMMAND_OUTPUT_TOO_LARGE`
- `PROCESS_START_FAILED`
- `PROCESS_KILL_FAILED`

## 21. 安全要求

服务端要求：

- 校验 token。
- 校验 device_id 归属当前用户。
- 只向在线且 enabled 的设备下发任务。
- 不向客户端下发未声明的 capability。
- 高风险能力应结合用户/组织策略判断。

客户端要求：

- 本地策略是最后防线。
- 所有路径必须规范化后再判断权限。
- 必须防止 `..` 和符号链接逃逸允许目录。
- 命令执行应受启用开关、cwd、超时和输出大小策略约束。
- 不在日志和错误中输出 token。
- 不默认读取敏感目录。

## 22. MVP 消息清单

第一版必须支持：

- `client.register`
- `client.ack`
- `server.registered`
- `client.ping`
- `server.pong`
- `client.goodbye`
- `task.request`
- `task.accepted`
- `task.rejected`
- `task.event`
- `task.result`
- `task.failed`
- `task.cancel`
- `task.cancelled`
- `server.reauth_required`
- `server.disconnect`
- `server.policy_updated`
- `error`

第一版必须支持的 capability：

- `system.info`
- `fs.stat`
- `fs.list`
- `fs.read`
- `fs.write`
- `fs.mkdir`
- `fs.move`
- `fs.delete`
- `fs.search`
- `process.exec`
- `process.cancel`
