//! JWT Token 管理
//!
//! 提供 JWT token 的生成和验证功能

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// JWT 密钥 (生产环境应从环境变量或配置文件读取)
const JWT_SECRET: &[u8] = b"qaexchange_jwt_secret_key_change_in_production";

/// Token 有效期 (秒) - 默认 24 小时
const TOKEN_EXPIRATION_SECS: u64 = 86400;

/// JWT Claims (载荷)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// 用户ID
    pub sub: String,

    /// 用户名
    pub username: String,

    /// 签发时间 (Unix timestamp)
    pub iat: u64,

    /// 过期时间 (Unix timestamp)
    pub exp: u64,
}

impl Claims {
    /// 创建新的 Claims
    pub fn new(user_id: String, username: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            sub: user_id,
            username,
            iat: now,
            exp: now + TOKEN_EXPIRATION_SECS,
        }
    }

    /// 检查 token 是否过期
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.exp < now
    }
}

/// 生成 JWT token
pub fn generate_token(user_id: &str, username: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = Claims::new(user_id.to_string(), username.to_string());

    let header = Header::new(Algorithm::HS256);
    let encoding_key = EncodingKey::from_secret(JWT_SECRET);

    encode(&header, &claims, &encoding_key)
}

/// 验证 JWT token 并返回 Claims
pub fn verify_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let decoding_key = DecodingKey::from_secret(JWT_SECRET);
    let validation = Validation::new(Algorithm::HS256);

    let token_data = decode::<Claims>(token, &decoding_key, &validation)?;

    Ok(token_data.claims)
}

/// 从 token 中提取用户ID (不验证签名，仅用于快速解析)
pub fn extract_user_id(token: &str) -> Option<String> {
    verify_token(token).ok().map(|claims| claims.sub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify_token() {
        let user_id = "user_123";
        let username = "testuser";

        // 生成 token
        let token = generate_token(user_id, username).unwrap();
        assert!(!token.is_empty());

        // 验证 token
        let claims = verify_token(&token).unwrap();
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.username, username);
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_invalid_token() {
        let result = verify_token("invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_user_id() {
        let token = generate_token("user_456", "john").unwrap();
        let user_id = extract_user_id(&token).unwrap();
        assert_eq!(user_id, "user_456");
    }

    #[test]
    fn test_token_expiration_check() {
        let token = generate_token("user_789", "alice").unwrap();
        let claims = verify_token(&token).unwrap();

        // Token 刚生成，不应该过期
        assert!(!claims.is_expired());

        // 验证过期时间设置正确
        assert!(claims.exp > claims.iat);
        assert_eq!(claims.exp - claims.iat, TOKEN_EXPIRATION_SECS);
    }

    #[test]
    fn test_tampered_token() {
        let token = generate_token("user_999", "bob").unwrap();

        // 篡改 token (替换最后一个字符)
        let mut tampered = token.clone();
        tampered.pop();
        tampered.push('X');

        // 验证应该失败
        let result = verify_token(&tampered);
        assert!(result.is_err());
    }
}
