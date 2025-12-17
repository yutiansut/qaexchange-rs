//! TLS 加密支持模块
//!
//! @yutiansut @quantaxis
//!
//! 提供 gRPC 节点间通信的 TLS 加密：
//! - 自签证书生成（开发/测试环境）
//! - CA 证书加载（生产环境）
//! - 双向 TLS（mTLS）认证
//! - 证书轮换支持
//!
//! 安全特性：
//! - TLS 1.3 默认启用
//! - 强加密套件（AES-256-GCM, ChaCha20-Poly1305）
//! - 证书链验证
//! - 主机名验证

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
    IsCa, KeyPair, KeyUsagePurpose, SanType,
};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tonic::transport::{Certificate, ClientTlsConfig, Identity, ServerTlsConfig};

// ═══════════════════════════════════════════════════════════════════════════
// 错误类型
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("Certificate generation failed: {0}")]
    CertGenError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Certificate parsing failed: {0}")]
    ParseError(String),

    #[error("TLS configuration failed: {0}")]
    ConfigError(String),

    #[error("Certificate expired")]
    CertificateExpired,

    #[error("Certificate not yet valid")]
    CertificateNotYetValid,

    #[error("Invalid certificate chain")]
    InvalidCertificateChain,
}

// ═══════════════════════════════════════════════════════════════════════════
// TLS 配置
// ═══════════════════════════════════════════════════════════════════════════

/// TLS 配置选项
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// 是否启用 TLS
    pub enabled: bool,
    /// CA 证书路径（PEM 格式）
    pub ca_cert_path: Option<PathBuf>,
    /// 服务端证书路径
    pub server_cert_path: Option<PathBuf>,
    /// 服务端私钥路径
    pub server_key_path: Option<PathBuf>,
    /// 客户端证书路径（mTLS）
    pub client_cert_path: Option<PathBuf>,
    /// 客户端私钥路径（mTLS）
    pub client_key_path: Option<PathBuf>,
    /// 是否要求客户端证书（mTLS）
    pub require_client_cert: bool,
    /// 自动生成自签证书（开发环境）
    pub auto_generate: bool,
    /// 证书存储目录
    pub cert_dir: PathBuf,
    /// 证书有效期（天）
    pub validity_days: u32,
    /// 域名/主机名列表
    pub domain_names: Vec<String>,
    /// IP 地址列表
    pub ip_addresses: Vec<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ca_cert_path: None,
            server_cert_path: None,
            server_key_path: None,
            client_cert_path: None,
            client_key_path: None,
            require_client_cert: false,
            auto_generate: true,
            cert_dir: PathBuf::from("certs"),
            validity_days: 365,
            domain_names: vec!["localhost".to_string()],
            ip_addresses: vec!["127.0.0.1".to_string(), "::1".to_string()],
        }
    }
}

impl TlsConfig {
    /// 创建生产环境配置
    pub fn production(
        ca_cert: impl Into<PathBuf>,
        server_cert: impl Into<PathBuf>,
        server_key: impl Into<PathBuf>,
    ) -> Self {
        Self {
            enabled: true,
            ca_cert_path: Some(ca_cert.into()),
            server_cert_path: Some(server_cert.into()),
            server_key_path: Some(server_key.into()),
            auto_generate: false,
            require_client_cert: true,
            ..Default::default()
        }
    }

    /// 创建开发环境配置（自动生成自签证书）
    pub fn development() -> Self {
        Self {
            enabled: true,
            auto_generate: true,
            require_client_cert: false,
            ..Default::default()
        }
    }

    /// 创建 mTLS 配置（双向认证）
    pub fn mtls(
        ca_cert: impl Into<PathBuf>,
        server_cert: impl Into<PathBuf>,
        server_key: impl Into<PathBuf>,
        client_cert: impl Into<PathBuf>,
        client_key: impl Into<PathBuf>,
    ) -> Self {
        Self {
            enabled: true,
            ca_cert_path: Some(ca_cert.into()),
            server_cert_path: Some(server_cert.into()),
            server_key_path: Some(server_key.into()),
            client_cert_path: Some(client_cert.into()),
            client_key_path: Some(client_key.into()),
            require_client_cert: true,
            auto_generate: false,
            ..Default::default()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 证书生成器
// ═══════════════════════════════════════════════════════════════════════════

/// 自签证书生成器
pub struct CertificateGenerator {
    config: TlsConfig,
}

/// CA 证书和密钥对（用于签发其他证书）
struct CaCert {
    cert: rcgen::Certificate,
    key_pair: KeyPair,
}

impl CertificateGenerator {
    pub fn new(config: TlsConfig) -> Self {
        Self { config }
    }

    /// 生成 CA 证书参数
    fn create_ca_params(&self, common_name: &str) -> Result<CertificateParams, TlsError> {
        let mut params = CertificateParams::new(vec![common_name.to_string()])
            .map_err(|e| TlsError::CertGenError(e.to_string()))?;

        // CA 证书配置
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        // 设置 Distinguished Name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, common_name);
        dn.push(DnType::OrganizationName, "QAExchange");
        dn.push(DnType::CountryName, "CN");
        params.distinguished_name = dn;

        // 有效期 (CA 有效期是普通证书的 10 倍)
        params.not_before = time::OffsetDateTime::now_utc();
        params.not_after =
            params.not_before + time::Duration::days(self.config.validity_days as i64 * 10);

        Ok(params)
    }

    /// 生成 CA 证书（返回 PEM 格式字符串）
    pub fn generate_ca(&self, common_name: &str) -> Result<(String, String), TlsError> {
        let params = self.create_ca_params(common_name)?;

        // 生成密钥对和证书
        let key_pair = KeyPair::generate().map_err(|e| TlsError::CertGenError(e.to_string()))?;
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| TlsError::CertGenError(e.to_string()))?;

        Ok((cert.pem(), key_pair.serialize_pem()))
    }

    /// 内部使用：生成 CA 证书对象（用于签发其他证书）
    fn generate_ca_internal(&self, common_name: &str) -> Result<CaCert, TlsError> {
        let params = self.create_ca_params(common_name)?;

        let key_pair = KeyPair::generate().map_err(|e| TlsError::CertGenError(e.to_string()))?;
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| TlsError::CertGenError(e.to_string()))?;

        Ok(CaCert { cert, key_pair })
    }

    /// 生成终端实体证书（服务器或客户端）
    fn generate_entity_cert(
        &self,
        ca: &CaCert,
        common_name: &str,
        is_server: bool,
    ) -> Result<(String, String), TlsError> {
        // 构建 SAN 名称列表
        let mut san_names: Vec<String> = self.config.domain_names.clone();
        if !san_names.contains(&common_name.to_string()) {
            san_names.push(common_name.to_string());
        }

        let mut params = CertificateParams::new(san_names.clone())
            .map_err(|e| TlsError::CertGenError(e.to_string()))?;

        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];

        // 根据用途设置 EKU
        if is_server {
            params.extended_key_usages = vec![
                ExtendedKeyUsagePurpose::ServerAuth,
                ExtendedKeyUsagePurpose::ClientAuth,
            ];
        } else {
            params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        }

        // 设置 SAN（Subject Alternative Names）
        let mut subject_alt_names = Vec::new();
        for name in &self.config.domain_names {
            if let Ok(dns_name) = name.clone().try_into() {
                subject_alt_names.push(SanType::DnsName(dns_name));
            }
        }
        for ip in &self.config.ip_addresses {
            if let Ok(addr) = ip.parse() {
                subject_alt_names.push(SanType::IpAddress(addr));
            }
        }
        params.subject_alt_names = subject_alt_names;

        // Distinguished Name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, common_name);
        dn.push(DnType::OrganizationName, "QAExchange");
        params.distinguished_name = dn;

        // 有效期
        params.not_before = time::OffsetDateTime::now_utc();
        params.not_after =
            params.not_before + time::Duration::days(self.config.validity_days as i64);

        // 生成密钥对和证书（由 CA 签发）
        let key_pair = KeyPair::generate().map_err(|e| TlsError::CertGenError(e.to_string()))?;
        let cert = params
            .signed_by(&key_pair, &ca.cert, &ca.key_pair)
            .map_err(|e| TlsError::CertGenError(e.to_string()))?;

        Ok((cert.pem(), key_pair.serialize_pem()))
    }

    /// 生成服务端证书（由内部生成的 CA 签发）
    pub fn generate_server_cert_standalone(
        &self,
        common_name: &str,
    ) -> Result<(String, String, String, String), TlsError> {
        // 首先生成 CA
        let ca = self.generate_ca_internal("QAExchange Internal CA")?;
        let ca_cert_pem = ca.cert.pem();
        let ca_key_pem = ca.key_pair.serialize_pem();

        // 生成服务端证书
        let (server_cert, server_key) = self.generate_entity_cert(&ca, common_name, true)?;

        Ok((ca_cert_pem, ca_key_pem, server_cert, server_key))
    }

    /// 生成并保存完整的证书链
    pub fn generate_and_save(&self) -> Result<CertificatePaths, TlsError> {
        // 创建证书目录
        fs::create_dir_all(&self.config.cert_dir)?;

        // 生成 CA 证书
        let ca = self.generate_ca_internal("QAExchange CA")?;
        let ca_cert_pem = ca.cert.pem();
        let ca_key_pem = ca.key_pair.serialize_pem();

        let ca_cert_path = self.config.cert_dir.join("ca.crt");
        let ca_key_path = self.config.cert_dir.join("ca.key");
        Self::write_pem(&ca_cert_path, &ca_cert_pem)?;
        Self::write_pem(&ca_key_path, &ca_key_pem)?;

        // 生成服务端证书
        let (server_cert, server_key) =
            self.generate_entity_cert(&ca, "qaexchange-server", true)?;

        let server_cert_path = self.config.cert_dir.join("server.crt");
        let server_key_path = self.config.cert_dir.join("server.key");
        Self::write_pem(&server_cert_path, &server_cert)?;
        Self::write_pem(&server_key_path, &server_key)?;

        // 生成客户端证书（用于 mTLS）
        let (client_cert, client_key) =
            self.generate_entity_cert(&ca, "qaexchange-client", false)?;

        let client_cert_path = self.config.cert_dir.join("client.crt");
        let client_key_path = self.config.cert_dir.join("client.key");
        Self::write_pem(&client_cert_path, &client_cert)?;
        Self::write_pem(&client_key_path, &client_key)?;

        log::info!(
            "Generated TLS certificates in {:?}",
            self.config.cert_dir
        );

        Ok(CertificatePaths {
            ca_cert: ca_cert_path,
            ca_key: ca_key_path,
            server_cert: server_cert_path,
            server_key: server_key_path,
            client_cert: client_cert_path,
            client_key: client_key_path,
        })
    }

    fn write_pem(path: &Path, content: &str) -> Result<(), TlsError> {
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        // 设置文件权限（仅 owner 可读）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(path, perms)?;
        }
        Ok(())
    }

    /// 静态方法：生成自签名证书到指定路径
    pub fn generate_self_signed(
        domain: &str,
        validity_days: u32,
        cert_path: &Path,
        key_path: &Path,
    ) -> Result<(), TlsError> {
        let config = TlsConfig {
            validity_days,
            domain_names: vec![domain.to_string()],
            ..Default::default()
        };

        let generator = Self::new(config);
        let (cert_pem, key_pem) = generator.generate_ca(domain)?;

        Self::write_pem(cert_path, &cert_pem)?;
        Self::write_pem(key_path, &key_pem)?;

        Ok(())
    }

    /// 静态方法：生成 CA 证书到指定路径
    pub fn generate_ca_to_file(
        common_name: &str,
        validity_days: u32,
        cert_path: &Path,
        key_path: &Path,
    ) -> Result<(), TlsError> {
        let config = TlsConfig {
            validity_days,
            ..Default::default()
        };

        let generator = Self::new(config);
        let (cert_pem, key_pem) = generator.generate_ca(common_name)?;

        Self::write_pem(cert_path, &cert_pem)?;
        Self::write_pem(key_path, &key_pem)?;

        Ok(())
    }

    /// 静态方法：使用指定 CA 生成签名证书
    pub fn generate_signed_certificate(
        common_name: &str,
        validity_days: u32,
        ca_cert_path: &Path,
        ca_key_path: &Path,
        cert_path: &Path,
        key_path: &Path,
    ) -> Result<(), TlsError> {
        let config = TlsConfig {
            validity_days,
            domain_names: vec![common_name.to_string(), "localhost".to_string()],
            ip_addresses: vec!["127.0.0.1".to_string()],
            ..Default::default()
        };

        // 读取 CA 密钥
        let ca_key_pem = fs::read_to_string(ca_key_path)?;
        let ca_key_pair =
            KeyPair::from_pem(&ca_key_pem).map_err(|e| TlsError::ParseError(e.to_string()))?;

        // 重新生成 CA 证书参数（rcgen 0.13 不支持直接解析 PEM 证书）
        let generator = Self::new(config.clone());
        let ca_params = generator.create_ca_params("QAExchange CA")?;
        let ca_cert = ca_params
            .self_signed(&ca_key_pair)
            .map_err(|e| TlsError::CertGenError(e.to_string()))?;

        let ca = CaCert {
            cert: ca_cert,
            key_pair: ca_key_pair,
        };

        // 生成实体证书
        let (cert_pem, key_pem) = generator.generate_entity_cert(&ca, common_name, true)?;

        Self::write_pem(cert_path, &cert_pem)?;
        Self::write_pem(key_path, &key_pem)?;

        Ok(())
    }
}

/// 证书路径集合
#[derive(Debug, Clone)]
pub struct CertificatePaths {
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
}

impl CertificatePaths {
    /// 从目录创建路径结构
    pub fn new(base_dir: &Path) -> Self {
        Self {
            ca_cert: base_dir.join("ca.crt"),
            ca_key: base_dir.join("ca.key"),
            server_cert: base_dir.join("server.crt"),
            server_key: base_dir.join("server.key"),
            client_cert: base_dir.join("client.crt"),
            client_key: base_dir.join("client.key"),
        }
    }

    /// 验证服务器证书文件存在
    pub fn validate_server_certs(&self) -> Result<(), TlsError> {
        if !self.ca_cert.exists() {
            return Err(TlsError::ConfigError("CA cert not found".into()));
        }
        if !self.server_cert.exists() {
            return Err(TlsError::ConfigError("Server cert not found".into()));
        }
        if !self.server_key.exists() {
            return Err(TlsError::ConfigError("Server key not found".into()));
        }
        Ok(())
    }

    /// 验证客户端证书文件存在
    pub fn validate_client_certs(&self) -> Result<(), TlsError> {
        if !self.ca_cert.exists() {
            return Err(TlsError::ConfigError("CA cert not found".into()));
        }
        if !self.client_cert.exists() {
            return Err(TlsError::ConfigError("Client cert not found".into()));
        }
        if !self.client_key.exists() {
            return Err(TlsError::ConfigError("Client key not found".into()));
        }
        Ok(())
    }

    /// 验证所有证书文件存在
    pub fn validate_all(&self) -> Result<(), TlsError> {
        self.validate_server_certs()?;
        self.validate_client_certs()?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TLS 配置构建器
// ═══════════════════════════════════════════════════════════════════════════

/// TLS 配置构建器
pub struct TlsConfigBuilder {
    config: TlsConfig,
}

impl TlsConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: TlsConfig::default(),
        }
    }

    /// 创建开发环境配置
    pub fn development(cert_dir: &Path) -> Result<TlsConfig, TlsError> {
        let config = TlsConfig {
            enabled: true,
            auto_generate: true,
            cert_dir: cert_dir.to_path_buf(),
            ..Default::default()
        };

        let generator = CertificateGenerator::new(config.clone());
        generator.generate_and_save()?;

        Ok(config)
    }

    /// 从配置文件加载或自动生成证书
    pub fn build(self) -> Result<(Option<ServerTlsConfig>, Option<ClientTlsConfig>), TlsError> {
        if !self.config.enabled {
            return Ok((None, None));
        }

        let paths = if self.config.auto_generate {
            // 检查证书是否已存在
            let ca_path = self.config.cert_dir.join("ca.crt");
            if ca_path.exists() {
                CertificatePaths::new(&self.config.cert_dir)
            } else {
                let generator = CertificateGenerator::new(self.config.clone());
                generator.generate_and_save()?
            }
        } else {
            CertificatePaths {
                ca_cert: self.config.ca_cert_path.clone().unwrap_or_default(),
                ca_key: self.config.cert_dir.join("ca.key"),
                server_cert: self.config.server_cert_path.clone().unwrap_or_default(),
                server_key: self.config.server_key_path.clone().unwrap_or_default(),
                client_cert: self.config.client_cert_path.clone().unwrap_or_default(),
                client_key: self.config.client_key_path.clone().unwrap_or_default(),
            }
        };

        // 加载证书
        let ca_cert = fs::read_to_string(&paths.ca_cert)?;
        let server_cert = fs::read_to_string(&paths.server_cert)?;
        let server_key = fs::read_to_string(&paths.server_key)?;

        // 构建服务端 TLS 配置
        let server_identity = Identity::from_pem(server_cert.clone(), server_key);
        let mut server_tls = ServerTlsConfig::new().identity(server_identity);

        if self.config.require_client_cert {
            let ca = Certificate::from_pem(&ca_cert);
            server_tls = server_tls.client_ca_root(ca);
        }

        // 构建客户端 TLS 配置
        let ca = Certificate::from_pem(&ca_cert);
        let mut client_tls = ClientTlsConfig::new().ca_certificate(ca);

        if paths.client_cert.exists() && paths.client_key.exists() {
            let client_cert = fs::read_to_string(&paths.client_cert)?;
            let client_key = fs::read_to_string(&paths.client_key)?;
            let client_identity = Identity::from_pem(client_cert, client_key);
            client_tls = client_tls.identity(client_identity);
        }

        // 设置域名（用于证书验证）
        if let Some(domain) = self.config.domain_names.first() {
            client_tls = client_tls.domain_name(domain);
        }

        Ok((Some(server_tls), Some(client_tls)))
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    pub fn auto_generate(mut self, auto: bool) -> Self {
        self.config.auto_generate = auto;
        self
    }

    pub fn cert_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.cert_dir = dir.into();
        self
    }

    pub fn with_ca_cert(mut self, path: impl AsRef<Path>) -> Self {
        self.config.ca_cert_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn with_cert(mut self, path: impl AsRef<Path>) -> Self {
        self.config.server_cert_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn with_key(mut self, path: impl AsRef<Path>) -> Self {
        self.config.server_key_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn with_verify_client(mut self, require: bool) -> Self {
        self.config.require_client_cert = require;
        self
    }

    pub fn domain_names(mut self, names: Vec<String>) -> Self {
        self.config.domain_names = names;
        self
    }

    pub fn ip_addresses(mut self, ips: Vec<String>) -> Self {
        self.config.ip_addresses = ips;
        self
    }

    pub fn validity_days(mut self, days: u32) -> Self {
        self.config.validity_days = days;
        self
    }
}

impl Default for TlsConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TlsConfig {
    /// 检查是否启用 mTLS
    pub fn verify_client(&self) -> bool {
        self.require_client_cert
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_ca() {
        let config = TlsConfig::default();
        let generator = CertificateGenerator::new(config);

        let (ca_cert, ca_key) = generator.generate_ca("Test CA").unwrap();

        assert!(ca_cert.contains("BEGIN CERTIFICATE"));
        assert!(ca_key.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_generate_server_cert_standalone() {
        let config = TlsConfig::default();
        let generator = CertificateGenerator::new(config);

        let (ca_cert, ca_key, server_cert, server_key) = generator
            .generate_server_cert_standalone("test-server")
            .unwrap();

        assert!(ca_cert.contains("BEGIN CERTIFICATE"));
        assert!(ca_key.contains("BEGIN PRIVATE KEY"));
        assert!(server_cert.contains("BEGIN CERTIFICATE"));
        assert!(server_key.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_generate_and_save() {
        let tmp_dir = tempdir().unwrap();
        let config = TlsConfig {
            enabled: true,
            auto_generate: true,
            cert_dir: tmp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let generator = CertificateGenerator::new(config);
        let paths = generator.generate_and_save().unwrap();

        assert!(paths.ca_cert.exists());
        assert!(paths.server_cert.exists());
        assert!(paths.server_key.exists());
        assert!(paths.client_cert.exists());
        assert!(paths.client_key.exists());
    }

    #[test]
    fn test_tls_config_builder() {
        let tmp_dir = tempdir().unwrap();

        let result = TlsConfigBuilder::new()
            .enabled(true)
            .auto_generate(true)
            .cert_dir(tmp_dir.path())
            .domain_names(vec!["localhost".to_string()])
            .build();

        assert!(result.is_ok());
        let (server_tls, client_tls) = result.unwrap();
        assert!(server_tls.is_some());
        assert!(client_tls.is_some());
    }

    #[test]
    fn test_tls_disabled() {
        let result = TlsConfigBuilder::new().enabled(false).build();

        assert!(result.is_ok());
        let (server_tls, client_tls) = result.unwrap();
        assert!(server_tls.is_none());
        assert!(client_tls.is_none());
    }

    #[test]
    fn test_certificate_paths() {
        let base_dir = PathBuf::from("/tmp/certs");
        let paths = CertificatePaths::new(&base_dir);

        assert_eq!(paths.ca_cert, base_dir.join("ca.crt"));
        assert_eq!(paths.server_cert, base_dir.join("server.crt"));
        assert_eq!(paths.client_cert, base_dir.join("client.crt"));
    }

    #[test]
    fn test_static_self_signed() {
        let tmp_dir = tempdir().unwrap();
        let cert_path = tmp_dir.path().join("test.crt");
        let key_path = tmp_dir.path().join("test.key");

        CertificateGenerator::generate_self_signed(
            "test.local",
            365,
            &cert_path,
            &key_path,
        )
        .unwrap();

        assert!(cert_path.exists());
        assert!(key_path.exists());
    }
}
