# Logline Agent

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
![License](https://img.shields.io/badge/License-Apache2-green.svg)

[简体中文](README.md) | English

A lightweight log streaming agent for real-time monitoring of log files and streaming log content to Logline server.

## Features

- **Lightweight & Efficient** - Single binary with low resource footprint
- **Real-time Monitoring** - Automatically detects file changes and streams new content
- **Auto Reconnection** - Automatically reconnects on network interruptions
- **Flexible Identification** - Supports custom device identifiers or automatic hostname usage
- **Multiple Startup Modes** - Supports reading from start, tail, or monitoring new content only
- **Simple Deployment** - No complex configuration required, works out of the box

## Installation

### Build from Source

```bash
# Clone the repository
git clone https://github.com/zibo-chen/logline-agent.git
cd logline-agent

# Build
cargo build --release

# Binary is located at target/release/logline-agent
```

### Install via cargo

```bash
cargo install --git https://github.com/zibo-chen/logline-agent
```

## Usage

### Basic Usage

```bash
# Monitor log file (defaults to sending last 64KB of existing content)
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log"

# Custom device identifier
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --device-id "prod-server-01"

# Read entire file from the beginning
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --from-start

# Monitor new content only (don't send existing content)
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --tail-bytes 0

# Custom tail size (send last 1MB of content)
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --tail-bytes 1048576

# Enable verbose logging
logline-agent --name "my-service" --server "192.168.1.10:12500" --file "/var/log/app.log" --verbose
```

### Command Line Arguments

| Argument | Short | Required | Default | Description |
|----------|-------|----------|---------|-------------|
| `--name` | `-n` | ✅ | - | Project/service name identifier |
| `--server` | `-s` | ❌ | `127.0.0.1:12500` | Logline server address (format: host:port) |
| `--file` | `-f` | ✅ | - | Path to the log file to monitor |
| `--device-id` | `-d` | ❌ | hostname | Device identifier |
| `--from-start` | - | ❌ | `false` | Read entire file from the beginning |
| `--tail-bytes` | `-t` | ❌ | `65536` | Send last N bytes of existing file (0 means don't send existing content) |
| `--verbose` | `-v` | ❌ | `false` | Enable verbose logging |

## Use Cases

### 1. Application Log Monitoring

```bash
# Monitor web application logs
logline-agent --name "web-app" --server "log-server:12500" --file "/var/log/nginx/access.log"
```

### 2. Microservices Log Collection

```bash
# Payment service
logline-agent --name "payment-service" --server "log-server:12500" --file "/app/logs/payment.log" --device-id "prod-payment-01"

# Order service
logline-agent --name "order-service" --server "log-server:12500" --file "/app/logs/order.log" --device-id "prod-order-01"
```

### 3. System Log Collection

```bash
# Monitor system logs
logline-agent --name "system-logs" --server "log-server:12500" --file "/var/log/syslog"
```

## How It Works

1. **File Monitoring** - Uses the `notify` library to monitor filesystem changes
2. **Incremental Reading** - Only reads and transmits new file content
3. **Protocol Transfer** - Uses custom Logline Protocol (LLP) for data transmission
4. **Auto Reconnection** - Automatically attempts to reconnect when connection is lost
5. **Unique Identification** - Generates unique Agent ID based on device ID and file path

### Logline Protocol (LLP)

The protocol uses a simple frame structure:

```
[Length: u32][Type: u8][Payload: bytes]
```

Message types:
- `0x01` - Handshake
- `0x02` - LogData
- `0xFF` - Keepalive

## License

Apache 2.0 License - See [LICENSE](LICENSE) file for details

## Related Projects

- [Logline APP](https://github.com/zibo-chen/logline) - Logline App implementation
