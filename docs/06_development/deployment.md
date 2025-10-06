# 部署指南

**版本**: v0.1.0
**更新日期**: 2025-10-03
**开发团队**: @yutiansut

---

## 📋 目录

1. [部署架构](#部署架构)
2. [系统要求](#系统要求)
3. [构建步骤](#构建步骤)
4. [单机部署](#单机部署)
5. [Docker 部署](#docker-部署)
6. [集群部署](#集群部署)
7. [配置说明](#配置说明)
8. [运维管理](#运维管理)
9. [故障排查](#故障排查)

---

## 部署架构

### 单机部署

```
┌─────────────────────────────────────┐
│         服务器 (4 核 16GB)           │
│                                     │
│  ┌────────────────────────────┐    │
│  │  qaexchange-rs             │    │
│  │  ├── HTTP Server :8080     │    │
│  │  └── WebSocket :8081       │    │
│  └────────────────────────────┘    │
│                                     │
│  ┌────────────────────────────┐    │
│  │  Nginx (反向代理)           │    │
│  │  ├── Port 80 → 8080        │    │
│  │  └── Port 443 → 8081       │    │
│  └────────────────────────────┘    │
│                                     │
│  ┌────────────────────────────┐    │
│  │  MongoDB (可选)             │    │
│  │  Port 27017                │    │
│  └────────────────────────────┘    │
└─────────────────────────────────────┘
```

**适用场景**: 开发、测试、小规模生产环境 (< 1000 用户)

### 集群部署

```
                    ┌──────────────┐
                    │ Load Balancer│
                    │  (Nginx/HAProxy)
                    └───────┬──────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
┌───────▼────────┐  ┌───────▼────────┐  ┌───────▼────────┐
│  Instance 1    │  │  Instance 2    │  │  Instance 3    │
│  HTTP:8080     │  │  HTTP:8080     │  │  HTTP:8080     │
│  WS:8081       │  │  WS:8081       │  │  WS:8081       │
└────────┬───────┘  └────────┬───────┘  └────────┬───────┘
         │                   │                   │
         └───────────────────┴───────────────────┘
                             │
                 ┌───────────▼──────────┐
                 │  Redis Cluster       │
                 │  (共享状态/会话)      │
                 └──────────────────────┘
                             │
                 ┌───────────▼──────────┐
                 │  MongoDB Replica Set │
                 │  (数据持久化)         │
                 └──────────────────────┘
```

**适用场景**: 大规模生产环境 (> 10K 用户)

---

## 系统要求

### 硬件要求

| 环境 | CPU | 内存 | 磁盘 | 网络 |
|------|-----|------|------|------|
| 开发 | 2 核 | 4GB | 20GB SSD | 100Mbps |
| 测试 | 4 核 | 8GB | 50GB SSD | 1Gbps |
| 生产 | 8 核+ | 16GB+ | 100GB+ SSD | 1Gbps+ |

### 软件要求

| 软件 | 版本 | 用途 |
|------|------|------|
| Rust | 1.75+ | 编译器 |
| Cargo | 1.75+ | 构建工具 |
| Linux | Ubuntu 20.04+ / CentOS 8+ | 操作系统 |
| Nginx | 1.18+ | 反向代理 (可选) |
| Docker | 20.10+ | 容器部署 (可选) |
| MongoDB | 5.0+ | 数据持久化 (可选) |
| Redis | 6.0+ | 缓存/会话 (可选) |

### 系统配置

**调整文件描述符限制**:
```bash
# /etc/security/limits.conf
* soft nofile 65536
* hard nofile 65536

# 临时调整
ulimit -n 65536
```

**调整网络参数**:
```bash
# /etc/sysctl.conf
net.core.somaxconn = 4096
net.ipv4.tcp_max_syn_backlog = 8192
net.ipv4.ip_local_port_range = 10000 65000
net.ipv4.tcp_tw_reuse = 1

# 生效
sudo sysctl -p
```

---

## 构建步骤

### 1. 克隆项目

```bash
git clone https://github.com/quantaxis/qaexchange-rs.git
cd qaexchange-rs
```

### 2. 安装 Rust

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 验证安装
rustc --version
cargo --version
```

### 3. 构建项目

**开发构建**:
```bash
cargo build
```

**生产构建**:
```bash
cargo build --release

# 二进制文件位于
ls target/release/qaexchange-rs
```

**优化编译**:
```bash
# 使用 LTO 和 CPU 优化
RUSTFLAGS="-C target-cpu=native -C lto=fat" cargo build --release
```

### 4. 运行测试

```bash
# 运行所有测试
cargo test --lib

# 运行特定测试
cargo test order_router --lib
```

### 5. 检查编译

```bash
cargo check --lib
```

---

## 单机部署

### 方式 1: 直接运行

**创建启动脚本**:
```bash
# start.sh
#!/bin/bash

export RUST_LOG=info
export QAEX_HTTP_PORT=8080
export QAEX_WS_PORT=8081

./target/release/qaexchange-rs
```

**后台运行**:
```bash
chmod +x start.sh
nohup ./start.sh > logs/app.log 2>&1 &

# 查看日志
tail -f logs/app.log

# 查看进程
ps aux | grep qaexchange-rs
```

### 方式 2: systemd 服务

**创建服务文件**:
```bash
# /etc/systemd/system/qaexchange.service
[Unit]
Description=QAEXCHANGE-RS Trading System
After=network.target

[Service]
Type=simple
User=quantaxis
WorkingDirectory=/home/quantaxis/qaexchange-rs
ExecStart=/home/quantaxis/qaexchange-rs/target/release/qaexchange-rs
Restart=on-failure
RestartSec=5s

Environment="RUST_LOG=info"
Environment="QAEX_HTTP_PORT=8080"
Environment="QAEX_WS_PORT=8081"

StandardOutput=journal
StandardError=journal
SyslogIdentifier=qaexchange

[Install]
WantedBy=multi-user.target
```

**启动服务**:
```bash
# 重载 systemd
sudo systemctl daemon-reload

# 启动服务
sudo systemctl start qaexchange

# 开机自启
sudo systemctl enable qaexchange

# 查看状态
sudo systemctl status qaexchange

# 查看日志
sudo journalctl -u qaexchange -f
```

### 方式 3: Nginx 反向代理

**配置文件**:
```nginx
# /etc/nginx/sites-available/qaexchange
upstream http_backend {
    server 127.0.0.1:8080;
}

upstream ws_backend {
    server 127.0.0.1:8081;
}

server {
    listen 80;
    server_name api.yourdomain.com;

    # HTTP API
    location /api/ {
        proxy_pass http://http_backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }

    # Health check
    location /health {
        proxy_pass http://http_backend;
    }
}

server {
    listen 80;
    server_name ws.yourdomain.com;

    # WebSocket
    location /ws {
        proxy_pass http://ws_backend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "Upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 86400s;  # 24h
    }
}
```

**启用配置**:
```bash
# 链接配置
sudo ln -s /etc/nginx/sites-available/qaexchange /etc/nginx/sites-enabled/

# 测试配置
sudo nginx -t

# 重启 Nginx
sudo systemctl restart nginx
```

### SSL/TLS 配置 (HTTPS/WSS)

**使用 Let's Encrypt**:
```bash
# 安装 certbot
sudo apt install certbot python3-certbot-nginx

# 自动配置 SSL
sudo certbot --nginx -d api.yourdomain.com -d ws.yourdomain.com

# 自动续期
sudo certbot renew --dry-run
```

**手动配置**:
```nginx
server {
    listen 443 ssl http2;
    server_name api.yourdomain.com;

    ssl_certificate /etc/ssl/certs/your_cert.pem;
    ssl_certificate_key /etc/ssl/private/your_key.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;

    location /api/ {
        proxy_pass http://http_backend;
        # ... 其他配置
    }
}
```

---

## Docker 部署

### Dockerfile

```dockerfile
# Dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .

# 构建 release 版本
RUN cargo build --release

# 运行时镜像
FROM debian:bookworm-slim

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 创建用户
RUN useradd -m -u 1000 quantaxis

WORKDIR /app

# 复制二进制文件
COPY --from=builder /app/target/release/qaexchange-rs .

# 切换用户
USER quantaxis

# 暴露端口
EXPOSE 8080 8081

# 启动命令
CMD ["./qaexchange-rs"]
```

### docker-compose.yml

```yaml
version: '3.8'

services:
  qaexchange:
    build: .
    container_name: qaexchange
    ports:
      - "8080:8080"
      - "8081:8081"
    environment:
      - RUST_LOG=info
      - QAEX_HTTP_PORT=8080
      - QAEX_WS_PORT=8081
    volumes:
      - ./logs:/app/logs
    restart: unless-stopped
    networks:
      - qanet

  nginx:
    image: nginx:alpine
    container_name: nginx
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
      - ./ssl:/etc/nginx/ssl:ro
    depends_on:
      - qaexchange
    restart: unless-stopped
    networks:
      - qanet

  mongodb:
    image: mongo:5
    container_name: mongodb
    ports:
      - "27017:27017"
    environment:
      - MONGO_INITDB_ROOT_USERNAME=admin
      - MONGO_INITDB_ROOT_PASSWORD=password123
    volumes:
      - mongodb_data:/data/db
    restart: unless-stopped
    networks:
      - qanet

  redis:
    image: redis:7-alpine
    container_name: redis
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data
    restart: unless-stopped
    networks:
      - qanet

networks:
  qanet:
    driver: bridge

volumes:
  mongodb_data:
  redis_data:
```

### 构建和运行

```bash
# 构建镜像
docker-compose build

# 启动服务
docker-compose up -d

# 查看日志
docker-compose logs -f qaexchange

# 停止服务
docker-compose down

# 重启服务
docker-compose restart qaexchange
```

---

## 集群部署

### 1. 负载均衡配置

**HAProxy 配置**:
```haproxy
# /etc/haproxy/haproxy.cfg
global
    maxconn 4096

defaults
    mode http
    timeout connect 5s
    timeout client 30s
    timeout server 30s

frontend http_front
    bind *:80
    default_backend http_back

backend http_back
    balance roundrobin
    server app1 192.168.1.101:8080 check
    server app2 192.168.1.102:8080 check
    server app3 192.168.1.103:8080 check

frontend ws_front
    bind *:8081
    default_backend ws_back

backend ws_back
    balance source  # 使用源 IP 哈希，确保同一客户端连接同一实例
    server app1 192.168.1.101:8081 check
    server app2 192.168.1.102:8081 check
    server app3 192.168.1.103:8081 check
```

### 2. Redis 共享会话

**未来实现** (需要代码修改):
```rust
// 将 WebSocket 会话状态存储到 Redis
let redis_client = redis::Client::open("redis://127.0.0.1/")?;
let mut con = redis_client.get_connection()?;

// 存储会话
con.set_ex::<_, _, ()>(
    format!("session:{}", session_id),
    serde_json::to_string(&session_data)?,
    3600  // 1小时过期
)?;

// 获取会话
let session_data: String = con.get(format!("session:{}", session_id))?;
```

### 3. 数据库集群

**MongoDB Replica Set**:
```bash
# 初始化副本集
mongo --eval '
rs.initiate({
  _id: "rs0",
  members: [
    { _id: 0, host: "mongo1:27017" },
    { _id: 1, host: "mongo2:27017" },
    { _id: 2, host: "mongo3:27017" }
  ]
})
'

# 连接字符串
mongodb://mongo1:27017,mongo2:27017,mongo3:27017/?replicaSet=rs0
```

---

## 配置说明

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `RUST_LOG` | info | 日志级别: trace/debug/info/warn/error |
| `QAEX_HTTP_PORT` | 8080 | HTTP API 端口 |
| `QAEX_WS_PORT` | 8081 | WebSocket 端口 |
| `QAEX_MONGO_URL` | - | MongoDB 连接字符串 (可选) |
| `QAEX_REDIS_URL` | - | Redis 连接字符串 (可选) |
| `QAEX_MAX_CONNECTIONS` | 10000 | 最大连接数 |

### 配置文件 (未来支持)

**config.toml**:
```toml
[server]
http_port = 8080
ws_port = 8081
max_connections = 10000

[risk]
max_position_ratio = 0.5
risk_ratio_warning = 0.8
risk_ratio_reject = 0.95
force_close_threshold = 1.0

[database]
mongodb_url = "mongodb://localhost:27017"
redis_url = "redis://localhost:6379"

[logging]
level = "info"
file = "logs/app.log"
```

---

## 运维管理

### 日志管理

**日志轮转**:
```bash
# /etc/logrotate.d/qaexchange
/home/quantaxis/qaexchange-rs/logs/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 0644 quantaxis quantaxis
}
```

### 监控指标

**系统指标**:
```bash
# CPU 使用率
top -p $(pgrep qaexchange-rs)

# 内存使用
ps aux | grep qaexchange-rs

# 网络连接
ss -tnp | grep qaexchange-rs

# 文件描述符
lsof -p $(pgrep qaexchange-rs) | wc -l
```

**应用指标** (未来集成 Prometheus):
```rust
// 暴露指标端点
#[get("/metrics")]
async fn metrics() -> String {
    format!(
        "# HELP orders_total Total orders submitted\n\
         # TYPE orders_total counter\n\
         orders_total {}\n",
        ORDER_COUNTER.load(Ordering::Relaxed)
    )
}
```

### 健康检查

```bash
# HTTP 健康检查
curl http://localhost:8080/health

# 预期响应
{"status":"ok","version":"0.1.0"}

# 集成到监控系统
watch -n 10 'curl -s http://localhost:8080/health'
```

### 备份策略

**数据备份**:
```bash
#!/bin/bash
# backup.sh

DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR=/backup

# MongoDB 备份
mongodump --uri="mongodb://localhost:27017/qaexchange" --out=$BACKUP_DIR/mongodb_$DATE

# 压缩
tar -czf $BACKUP_DIR/mongodb_$DATE.tar.gz $BACKUP_DIR/mongodb_$DATE
rm -rf $BACKUP_DIR/mongodb_$DATE

# 保留最近 7 天
find $BACKUP_DIR -name "mongodb_*.tar.gz" -mtime +7 -delete
```

**定时备份**:
```bash
# crontab -e
0 2 * * * /home/quantaxis/backup.sh
```

---

## 故障排查

### 问题 1: 服务无法启动

**检查端口占用**:
```bash
sudo lsof -i :8080
sudo lsof -i :8081

# 杀死占用进程
sudo kill -9 <PID>
```

**检查日志**:
```bash
# systemd 日志
sudo journalctl -u qaexchange -n 100

# 应用日志
tail -f logs/app.log
```

### 问题 2: WebSocket 连接失败

**检查 Nginx 配置**:
```bash
# 确保有 WebSocket 支持
proxy_http_version 1.1;
proxy_set_header Upgrade $http_upgrade;
proxy_set_header Connection "Upgrade";
```

**检查防火墙**:
```bash
# 开放端口
sudo ufw allow 8080/tcp
sudo ufw allow 8081/tcp
```

### 问题 3: 性能下降

**检查资源使用**:
```bash
# CPU
mpstat 1 5

# 内存
free -m

# 磁盘 IO
iostat -x 1 5

# 网络
iftop
```

**调整系统参数**:
```bash
# 增加文件描述符
ulimit -n 65536

# 调整 TCP 参数
sudo sysctl -w net.ipv4.tcp_tw_reuse=1
```

### 问题 4: 内存泄漏

**监控内存**:
```bash
# 持续监控
watch -n 5 'ps aux | grep qaexchange-rs'

# 使用 Valgrind (需要 debug 构建)
valgrind --leak-check=full ./target/debug/qaexchange-rs
```

---

## 升级流程

### 滚动升级 (零停机)

```bash
# 1. 构建新版本
cargo build --release

# 2. 停止一个实例
sudo systemctl stop qaexchange@1

# 3. 替换二进制文件
cp target/release/qaexchange-rs /opt/qaexchange/bin/

# 4. 启动实例
sudo systemctl start qaexchange@1

# 5. 重复步骤 2-4 升级其他实例
```

### 回滚流程

```bash
# 保留旧版本
cp target/release/qaexchange-rs target/release/qaexchange-rs.backup

# 回滚
cp target/release/qaexchange-rs.backup target/release/qaexchange-rs
sudo systemctl restart qaexchange
```

---

## 安全加固

### 1. 系统安全

```bash
# 禁用 root 登录
sudo sed -i 's/PermitRootLogin yes/PermitRootLogin no/' /etc/ssh/sshd_config
sudo systemctl restart sshd

# 启用防火墙
sudo ufw enable
sudo ufw allow 22/tcp
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
```

### 2. 应用安全

- 使用非 root 用户运行
- 环境变量存储敏感信息
- 启用 HTTPS/WSS
- 实现 Token 认证
- 限流防止 DDoS

### 3. 数据安全

- 数据库启用认证
- 定期备份
- 加密传输
- 访问日志记录

---

**文档更新**: 2025-10-03
**维护者**: @yutiansut
