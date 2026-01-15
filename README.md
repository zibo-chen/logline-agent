# Logline Agent

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
![License](https://img.shields.io/badge/License-Apache2-green.svg)

简体中文 | [English](README.en.md)

轻量级日志流代理工具，用于实时监控日志文件并将日志内容流式传输到 Logline 服务器。

## 特性

- **轻量高效** - 单一二进制文件，低资源占用
- **实时监控** - 自动检测文件变化并流式传输新内容
- **自动重连** - 网络中断时自动重新连接
- **灵活标识** - 支持自定义设备标识符或自动使用主机名
- **多种启动模式** - 支持从头读取、读取尾部或仅监控新增内容
- **简单部署** - 无需复杂配置，开箱即用

## 安装

### 从源码编译

```bash
# 克隆仓库
git clone https://github.com/zibo-chen/logline-agent.git
cd logline-agent

# 编译
cargo build --release

# 二进制文件位于 target/release/logline-agent
```

### 使用 cargo install

```bash
cargo install --git https://github.com/zibo-chen/logline-agent
```

## 使用方法

### 基本用法

```bash
# 监控日志文件（默认发送最后 64KB 的现有内容）
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log"

# 自定义设备标识符
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --device-id "prod-server-01"

# 从文件开头读取全部内容
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --from-start

# 仅监控新增内容（不发送现有内容）
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --tail-bytes 0

# 自定义尾部大小（发送最后 1MB 的内容）
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --tail-bytes 1048576

# 启用详细日志输出
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --verbose
```

### 命令行参数

| 参数 | 缩写 | 必需 | 默认值 | 说明 |
|------|------|------|--------|------|
| `--name` | `-n` | ✅ | - | 项目/服务名称标识符 |
| `--server` | `-s` | ❌ | `127.0.0.1:12500` | Logline 服务器地址（格式：host:port） |
| `--file` | `-f` | ✅ | - | 要监控的日志文件路径 |
| `--device-id` | `-d` | ❌ | 主机名 | 设备标识符 |
| `--from-start` | - | ❌ | `false` | 从文件开头读取全部内容 |
| `--tail-bytes` | `-t` | ❌ | `65536` | 发送现有文件的最后 N 字节（0 表示不发送现有内容） |
| `--verbose` | `-v` | ❌ | `false` | 启用详细日志输出 |

## 应用场景

### 1. 应用日志监控

```bash
# 监控 Web 应用日志
logline-agent --name "web-app" --server "log-server:12500" --file "/var/log/nginx/access.log"
```

### 2. 微服务日志收集

```bash
# 支付服务
logline-agent --name "payment-service" --server "log-server:12500" --file "/app/logs/payment.log" --device-id "prod-payment-01"

# 订单服务
logline-agent --name "order-service" --server "log-server:12500" --file "/app/logs/order.log" --device-id "prod-order-01"
```

### 3. 系统日志收集

```bash
# 监控系统日志
logline-agent --name "system-logs" --server "log-server:12500" --file "/var/log/syslog"
```

## 工作原理

1. **文件监控** - 使用 `notify` 库监控文件系统变化
2. **增量读取** - 仅读取和传输文件的新增内容
3. **协议传输** - 使用自定义的 Logline Protocol (LLP) 进行数据传输
4. **自动重连** - 连接断开时自动尝试重新连接
5. **唯一标识** - 根据设备 ID 和文件路径生成唯一的 Agent ID

### Logline Protocol (LLP)

协议采用简单的帧结构：

```
[Length: u32][Type: u8][Payload: bytes]
```

消息类型：
- `0x01` - Handshake（握手）
- `0x02` - LogData（日志数据）
- `0xFF` - Keepalive（心跳保活）

[text](../logline/LICENSE)
## 许可证

Apache 2.0 License - 详见 [LICENSE](LICENSE) 文件


## 相关项目

- [Logline APP](https://github.com/zibo-chen/logline) - Logline 应用端实现

