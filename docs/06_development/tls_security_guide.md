# TLS/mTLS 安全配置指南

> **版本**: v1.0.0
> **最后更新**: 2025-12-18
> **维护者**: @yutiansut @quantaxis

## 目录

- [概述](#概述)
- [快速开始](#快速开始)
- [证书生成](#证书生成)
- [TLS 配置](#tls-配置)
- [mTLS 双向认证](#mtls-双向认证)
- [生产部署](#生产部署)
- [故障排查](#故障排查)

---

## 概述

QAExchange-RS 使用 TLS (Transport Layer Security) 保护网络通信安全。本指南涵盖：

- **TLS**: 服务端证书验证，保护数据传输
- **mTLS**: 双向证书验证，客户端身份认证

### 依赖组件

| 组件 | 版本 | 用途 |
|------|------|------|
| rcgen | 0.13 | 证书生成 |
| rustls | 0.23 | TLS 实现 |
| rustls-pemfile | 2.2 | PEM 解析 |
| tokio-rustls | 0.26 | 异步 TLS |

### 安全特性

- ✅ TLS 1.3 默认启用
- ✅ 强密码套件（AES-256-GCM, ChaCha20-Poly1305）
- ✅ 证书链验证
- ✅ 主机名验证
- ✅ 客户端证书认证（mTLS）

---

## 快速开始

### 1. 生成开发证书

```rust
use qaexchange::replication::tls::CertificateGenerator;

// 生成自签名 CA
let ca = CertificateGenerator::generate_ca_certificate(
    "QAExchange Dev CA",
    365 * 10,  // 10 年
)?;

// 生成服务器证书
let server_cert = CertificateGenerator::generate_server_certificate(
    &ca,
    "localhost",
    &["localhost", "127.0.0.1"],
    365,
)?;

// 保存证书
std::fs::write("ca.pem", &ca.cert_pem)?;
std::fs::write("server.pem", &server_cert.cert_pem)?;
std::fs::write("server.key", &server_cert.key_pem)?;
```

### 2. 配置 TLS 服务端

```rust
use qaexchange::replication::tls::{TlsConfigBuilder, CertificatePaths};

let paths = CertificatePaths {
    cert_path: "server.pem".into(),
    key_path: "server.key".into(),
};

let server_config = TlsConfigBuilder::new()
    .with_certificate_paths(&paths)?
    .build_server_config()?;
```

### 3. 配置 TLS 客户端

```rust
let ca_paths = CertificatePaths {
    cert_path: "ca.pem".into(),
    key_path: String::new(),  // CA 不需要私钥
};

let client_config = TlsConfigBuilder::new()
    .with_ca_certificate(&ca_paths)?
    .build_client_config()?;
```

---

## 证书生成

### CertificateGenerator API

#### 生成 CA 证书

```rust
/// 生成 CA 根证书
pub fn generate_ca_certificate(
    common_name: &str,
    validity_days: u32,
) -> Result<CertificatePaths, TlsError>
```

**参数**:
- `common_name`: CA 名称，例如 "QAExchange Root CA"
- `validity_days`: 有效期天数，建议 CA 证书 10 年

**返回**:
- `CertificatePaths`: 包含 PEM 格式的证书和私钥

**示例**:

```rust
let ca = CertificateGenerator::generate_ca_certificate(
    "QAExchange Production CA",
    365 * 10,
)?;

// ca.cert_pem - CA 证书 (分发给客户端)
// ca.key_pem  - CA 私钥 (严格保密)
```

#### 生成服务器证书

```rust
/// 生成服务器证书
pub fn generate_server_certificate(
    ca: &CaCert,
    common_name: &str,
    san_names: &[&str],
    validity_days: u32,
) -> Result<CertificatePaths, TlsError>
```

**参数**:
- `ca`: CA 证书（用于签名）
- `common_name`: 服务器名称
- `san_names`: Subject Alternative Names（DNS 名称和 IP 地址）
- `validity_days`: 有效期天数，建议 1 年

**示例**:

```rust
let server = CertificateGenerator::generate_server_certificate(
    &ca,
    "exchange-server-01",
    &[
        "exchange.quantaxis.io",    // 域名
        "*.exchange.quantaxis.io",  // 通配符
        "localhost",                // 本地开发
        "192.168.1.100",            // IP 地址
    ],
    365,
)?;
```

#### 生成客户端证书

```rust
/// 生成客户端证书 (用于 mTLS)
pub fn generate_client_certificate(
    ca: &CaCert,
    common_name: &str,
    validity_days: u32,
) -> Result<CertificatePaths, TlsError>
```

**参数**:
- `ca`: CA 证书（用于签名）
- `common_name`: 客户端标识，例如 "trader-001"
- `validity_days`: 有效期天数

**示例**:

```rust
let client = CertificateGenerator::generate_client_certificate(
    &ca,
    "trader-001",
    365,
)?;

// 分发给客户端:
// - client.cert_pem (客户端证书)
// - client.key_pem  (客户端私钥)
// - ca.cert_pem     (CA 证书，用于验证服务端)
```

### 证书链结构

```
┌─────────────────────────────────────────────────────────────┐
│                    Certificate Chain                         │
│                                                              │
│   ┌─────────────────────────────────────────────────────┐   │
│   │                    Root CA                            │   │
│   │                                                       │   │
│   │   Subject: CN=QAExchange Root CA                     │   │
│   │   Issuer:  CN=QAExchange Root CA  (self-signed)      │   │
│   │   Valid:   10 years                                   │   │
│   │   Usage:   Certificate Signing                        │   │
│   │                                                       │   │
│   │   ⚠️  私钥离线存储，仅用于签发证书                    │   │
│   └───────────────────────┬─────────────────────────────┘   │
│                           │ signs                            │
│                           ▼                                  │
│   ┌─────────────────────────────────────────────────────┐   │
│   │                 Server Certificate                    │   │
│   │                                                       │   │
│   │   Subject: CN=exchange.quantaxis.io                  │   │
│   │   Issuer:  CN=QAExchange Root CA                     │   │
│   │   Valid:   1 year                                     │   │
│   │   SAN:     DNS:exchange.quantaxis.io,                │   │
│   │            DNS:localhost,                             │   │
│   │            IP:192.168.1.100                          │   │
│   │   Usage:   Server Authentication                      │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                              │
│   ┌─────────────────────────────────────────────────────┐   │
│   │                 Client Certificate                    │   │
│   │                                                       │   │
│   │   Subject: CN=trader-001                             │   │
│   │   Issuer:  CN=QAExchange Root CA                     │   │
│   │   Valid:   1 year                                     │   │
│   │   Usage:   Client Authentication                      │   │
│   └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## TLS 配置

### TlsConfigBuilder

```rust
/// TLS 配置构建器
pub struct TlsConfigBuilder {
    cert_chain: Option<Vec<CertificateDer<'static>>>,
    key: Option<PrivateKeyDer<'static>>,
    ca_certs: Vec<CertificateDer<'static>>,
    require_client_auth: bool,
}
```

### 服务端配置

#### 基础 TLS（仅服务端证书）

```rust
use qaexchange::replication::tls::{TlsConfigBuilder, CertificatePaths};

let server_paths = CertificatePaths {
    cert_path: "/etc/qaexchange/server.pem".into(),
    key_path: "/etc/qaexchange/server.key".into(),
};

let config = TlsConfigBuilder::new()
    .with_certificate_paths(&server_paths)?
    .build_server_config()?;

// 用于 tonic gRPC
let tls_config = tonic::transport::ServerTlsConfig::new()
    .with_rustls_server_config(config);
```

#### mTLS（双向证书验证）

```rust
let ca_paths = CertificatePaths {
    cert_path: "/etc/qaexchange/ca.pem".into(),
    key_path: String::new(),
};

let config = TlsConfigBuilder::new()
    .with_certificate_paths(&server_paths)?
    .require_client_auth(&ca_paths)?  // 启用客户端证书验证
    .build_server_config()?;
```

### 客户端配置

#### 基础 TLS（验证服务端证书）

```rust
let ca_paths = CertificatePaths {
    cert_path: "/etc/qaexchange/ca.pem".into(),
    key_path: String::new(),
};

let config = TlsConfigBuilder::new()
    .with_ca_certificate(&ca_paths)?
    .build_client_config()?;

// 用于 tonic gRPC
let tls_config = tonic::transport::ClientTlsConfig::new()
    .with_rustls_client_config(config);
```

#### mTLS（提供客户端证书）

```rust
let client_paths = CertificatePaths {
    cert_path: "/etc/qaexchange/client.pem".into(),
    key_path: "/etc/qaexchange/client.key".into(),
};

let ca_paths = CertificatePaths {
    cert_path: "/etc/qaexchange/ca.pem".into(),
    key_path: String::new(),
};

let config = TlsConfigBuilder::new()
    .with_certificate_paths(&client_paths)?  // 客户端证书
    .with_ca_certificate(&ca_paths)?         // CA 证书
    .build_client_config()?;
```

---

## mTLS 双向认证

### 工作流程

```
┌─────────────┐                              ┌─────────────┐
│   Client    │                              │   Server    │
└──────┬──────┘                              └──────┬──────┘
       │                                            │
       │ ──────── ClientHello ─────────────────────>│
       │                                            │
       │ <─────── ServerHello ──────────────────────│
       │ <─────── Server Certificate ───────────────│
       │ <─────── Certificate Request ──────────────│  ← mTLS
       │ <─────── ServerHelloDone ──────────────────│
       │                                            │
       │  (验证服务端证书)                           │
       │                                            │
       │ ──────── Client Certificate ──────────────>│  ← mTLS
       │ ──────── ClientKeyExchange ───────────────>│
       │ ──────── CertificateVerify ───────────────>│  ← mTLS
       │ ──────── Finished ────────────────────────>│
       │                                            │
       │                     (验证客户端证书)        │
       │                                            │
       │ <─────── Finished ─────────────────────────│
       │                                            │
       │ ═══════ Encrypted Application Data ═══════ │
```

### 配置示例

**服务端**:

```rust
// 配置 mTLS 服务端
let server_config = TlsConfigBuilder::new()
    .with_certificate_paths(&server_paths)?
    .require_client_auth(&ca_paths)?
    .build_server_config()?;

// 获取客户端身份
fn extract_client_identity(tls_info: &TlsInfo) -> Option<String> {
    tls_info
        .peer_certificates()
        .and_then(|certs| certs.first())
        .and_then(|cert| {
            // 解析证书获取 CN
            parse_common_name(cert)
        })
}
```

**客户端**:

```rust
// 配置 mTLS 客户端
let client_config = TlsConfigBuilder::new()
    .with_certificate_paths(&client_paths)?
    .with_ca_certificate(&ca_paths)?
    .build_client_config()?;

// 连接服务器
let channel = Channel::from_static("https://exchange.quantaxis.io:8443")
    .tls_config(client_config)?
    .connect()
    .await?;
```

### 身份映射

mTLS 证书的 Common Name (CN) 可用于身份映射：

```rust
/// 从证书提取用户身份
fn authenticate_client(cert: &Certificate) -> Result<UserId, AuthError> {
    let cn = extract_common_name(cert)?;

    // 映射规则示例
    match cn.as_str() {
        "admin" => Ok(UserId::Admin),
        cn if cn.starts_with("trader-") => {
            let id = cn.strip_prefix("trader-").unwrap();
            Ok(UserId::Trader(id.to_string()))
        }
        cn if cn.starts_with("service-") => {
            Ok(UserId::Service(cn.to_string()))
        }
        _ => Err(AuthError::UnknownClient(cn)),
    }
}
```

---

## 生产部署

### 目录结构

```
/etc/qaexchange/
├── ca/
│   ├── ca.pem          # CA 证书 (公开)
│   └── ca.key          # CA 私钥 (离线存储)
├── server/
│   ├── server.pem      # 服务器证书
│   └── server.key      # 服务器私钥 (权限 600)
└── clients/
    ├── trader-001/
    │   ├── client.pem  # 客户端证书
    │   └── client.key  # 客户端私钥
    └── trader-002/
        ├── client.pem
        └── client.key
```

### 权限设置

```bash
# CA 证书 (公开)
chmod 644 /etc/qaexchange/ca/ca.pem

# CA 私钥 (严格保密)
chmod 600 /etc/qaexchange/ca/ca.key
chown root:root /etc/qaexchange/ca/ca.key

# 服务器私钥
chmod 600 /etc/qaexchange/server/server.key
chown qaexchange:qaexchange /etc/qaexchange/server/server.key

# 客户端证书 (发送给对应用户)
chmod 600 /etc/qaexchange/clients/*/client.key
```

### 证书轮换

#### 自动轮换脚本

```bash
#!/bin/bash
# /etc/qaexchange/scripts/rotate-certs.sh

CA_CERT="/etc/qaexchange/ca/ca.pem"
CA_KEY="/etc/qaexchange/ca/ca.key"
SERVER_CERT="/etc/qaexchange/server/server.pem"
SERVER_KEY="/etc/qaexchange/server/server.key"

# 检查证书过期时间
days_until_expiry() {
    local cert=$1
    local expiry=$(openssl x509 -enddate -noout -in "$cert" | cut -d= -f2)
    local expiry_epoch=$(date -d "$expiry" +%s)
    local now_epoch=$(date +%s)
    echo $(( (expiry_epoch - now_epoch) / 86400 ))
}

# 如果证书将在 30 天内过期，则轮换
if [ $(days_until_expiry "$SERVER_CERT") -lt 30 ]; then
    echo "Rotating server certificate..."

    # 生成新证书 (使用 qaexchange CLI)
    qaexchange-cli cert generate-server \
        --ca-cert "$CA_CERT" \
        --ca-key "$CA_KEY" \
        --output "$SERVER_CERT.new" \
        --key-output "$SERVER_KEY.new" \
        --days 365

    # 备份旧证书
    mv "$SERVER_CERT" "$SERVER_CERT.bak"
    mv "$SERVER_KEY" "$SERVER_KEY.bak"

    # 使用新证书
    mv "$SERVER_CERT.new" "$SERVER_CERT"
    mv "$SERVER_KEY.new" "$SERVER_KEY"

    # 重载服务
    systemctl reload qaexchange
fi
```

#### Cron 定时任务

```cron
# /etc/cron.d/qaexchange-cert-rotation
0 0 * * * root /etc/qaexchange/scripts/rotate-certs.sh >> /var/log/qaexchange/cert-rotation.log 2>&1
```

### 监控告警

```yaml
# Prometheus 告警规则
groups:
  - name: tls
    rules:
      - alert: CertificateExpiringSoon
        expr: qaexchange_tls_cert_expiry_days < 30
        for: 1h
        labels:
          severity: warning
        annotations:
          summary: "TLS certificate expiring in {{ $value }} days"

      - alert: CertificateExpired
        expr: qaexchange_tls_cert_expiry_days < 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "TLS certificate has expired!"
```

---

## 故障排查

### 常见错误

#### 1. 证书验证失败

**错误信息**:
```
Error: TLS handshake failed: certificate verify failed
```

**原因**:
- 客户端没有信任服务端的 CA
- 证书链不完整
- 证书已过期

**解决方案**:
```bash
# 检查证书有效期
openssl x509 -in server.pem -noout -dates

# 验证证书链
openssl verify -CAfile ca.pem server.pem

# 检查证书详情
openssl x509 -in server.pem -noout -text
```

#### 2. 主机名不匹配

**错误信息**:
```
Error: hostname verification failed
```

**原因**:
- 证书的 SAN (Subject Alternative Names) 不包含连接的主机名

**解决方案**:
```bash
# 检查 SAN
openssl x509 -in server.pem -noout -text | grep -A1 "Subject Alternative Name"

# 重新生成证书，包含正确的 SAN
CertificateGenerator::generate_server_certificate(
    &ca,
    "server",
    &["exchange.example.com", "192.168.1.100"],  // 添加所有访问地址
    365,
)?;
```

#### 3. 客户端证书被拒绝

**错误信息**:
```
Error: client certificate required but not provided
```

**原因**:
- 服务端配置了 mTLS，但客户端没有提供证书

**解决方案**:
```rust
// 确保客户端配置了证书
let config = TlsConfigBuilder::new()
    .with_certificate_paths(&client_paths)?  // 必须提供
    .with_ca_certificate(&ca_paths)?
    .build_client_config()?;
```

#### 4. 私钥格式错误

**错误信息**:
```
Error: invalid private key format
```

**原因**:
- 私钥不是 PKCS#8 或 RSA 格式
- 私钥文件损坏

**解决方案**:
```bash
# 检查私钥格式
head -1 server.key
# 应该是 "-----BEGIN PRIVATE KEY-----" (PKCS#8)
# 或 "-----BEGIN RSA PRIVATE KEY-----" (RSA)

# 转换格式
openssl pkcs8 -topk8 -nocrypt -in server.key -out server.pkcs8.key
```

### 调试工具

#### OpenSSL 测试连接

```bash
# 测试 TLS 连接
openssl s_client -connect exchange.quantaxis.io:8443 \
    -CAfile ca.pem \
    -showcerts

# 测试 mTLS 连接
openssl s_client -connect exchange.quantaxis.io:8443 \
    -CAfile ca.pem \
    -cert client.pem \
    -key client.key
```

#### 证书信息查看

```bash
# 查看证书详情
openssl x509 -in server.pem -noout -text

# 查看证书指纹
openssl x509 -in server.pem -noout -fingerprint -sha256

# 验证证书与私钥匹配
openssl x509 -noout -modulus -in server.pem | openssl md5
openssl rsa -noout -modulus -in server.key | openssl md5
# 两个 MD5 值应该相同
```

---

## 附录

### A. 证书字段说明

| 字段 | 说明 | 示例 |
|------|------|------|
| CN | Common Name，证书持有者名称 | "QAExchange Server" |
| O | Organization，组织名称 | "QUANTAXIS" |
| OU | Organizational Unit，部门 | "Engineering" |
| C | Country，国家代码 | "CN" |
| ST | State，省/州 | "Zhejiang" |
| L | Locality，城市 | "Hangzhou" |
| SAN | Subject Alternative Names | DNS:*.example.com |

### B. 密码套件

QAExchange 默认使用的 TLS 1.3 密码套件：

| 密码套件 | 安全级别 |
|----------|----------|
| TLS_AES_256_GCM_SHA384 | 高 |
| TLS_AES_128_GCM_SHA256 | 高 |
| TLS_CHACHA20_POLY1305_SHA256 | 高 |

### C. 参考资料

- [rustls 文档](https://docs.rs/rustls/)
- [rcgen 文档](https://docs.rs/rcgen/)
- [RFC 8446 - TLS 1.3](https://tools.ietf.org/html/rfc8446)
- [RFC 5280 - X.509 证书](https://tools.ietf.org/html/rfc5280)

---

**文档版本**: v1.0.0
**最后更新**: 2025-12-18
**维护者**: @yutiansut @quantaxis
