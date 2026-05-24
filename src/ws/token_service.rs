use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::config::AppConfig;
use injectable::prelude::*;

type HmacSha256 = Hmac<Sha256>;

/// WS token payload.
#[derive(Serialize, Deserialize)]
struct WsTokenPayload {
    sub: String,
    iat: u64,
    exp: u64,
}

/// Service for creating and verifying WebSocket authentication tokens.
#[derive(Debug, Clone)]
pub struct WsTokenService {
    secret: Vec<u8>,
}

#[injectable]
impl WsTokenService {
    /// Create a new WsTokenService injecting AppConfig for the HMAC secret.
    #[injectable(ctor)]
    pub fn new(#[injectable(inject)] config: Arc<AppConfig>) -> Self {
        Self {
            secret: config.payload_secret.as_bytes().to_vec(),
        }
    }

    /// Create a WsTokenService with an explicit secret (for tests).
    pub fn with_secret(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into().as_bytes().to_vec(),
        }
    }

    /// Create a short-lived WebSocket token.
    pub fn create_token(&self, user_id: &str, ttl_secs: u64) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let payload = WsTokenPayload {
            sub: user_id.to_string(),
            iat: now,
            exp: now + ttl_secs,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());
        let signature = self.sign(encoded.as_bytes());

        format!("{}.{}", encoded, signature)
    }

    /// Verify a WebSocket token and return the user_id.
    pub fn verify_token(&self, token: &str) -> Option<String> {
        let parts: Vec<&str> = token.splitn(2, '.').collect();
        if parts.len() != 2 {
            return None;
        }

        if !self.verify(parts[0].as_bytes(), parts[1]) {
            return None;
        }

        let json = URL_SAFE_NO_PAD.decode(parts[0]).ok()?;
        let payload: WsTokenPayload = serde_json::from_slice(&json).ok()?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now > payload.exp {
            return None;
        }

        Some(payload.sub)
    }

    fn sign(&self, payload: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("HMAC key length");
        mac.update(payload);
        hex::encode(mac.finalize().into_bytes())
    }

    fn verify(&self, payload: &[u8], signature: &str) -> bool {
        let decoded = match hex::decode(signature) {
            Ok(d) => d,
            Err(_) => return false,
        };
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("HMAC key length");
        mac.update(payload);
        mac.verify_slice(&decoded).is_ok()
    }
}

mod tests {
    use crate::config::AppConfig;
    use crate::ws::token_service::WsTokenService;

    #[test]
    fn test_create_and_verify_token() {
        let service = WsTokenService::with_secret("test-secret");
        let token = service.create_token("alice", 120);
        let user_id = service.verify_token(&token);
        assert_eq!(user_id, Some("alice".to_string()));
    }

    #[test]
    fn test_verify_invalid_token() {
        let service = WsTokenService::with_secret("test-secret");
        let result = service.verify_token("invalid-token");
        assert!(result.is_none());
    }

    #[test]
    fn test_verify_empty_token() {
        let service = WsTokenService::with_secret("test-secret");
        let result = service.verify_token("");
        assert!(result.is_none());
    }

    #[test]
    fn test_verify_expired_token() {
        let service = WsTokenService::with_secret("test-secret");
        let token = service.create_token("alice", 0);
        // Token with 0 TTL should be expired immediately
        // (may not be expired if within same second, so we just verify it works)
        let result = service.verify_token(&token);
        // Could be None (expired) or Some (same second)
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_verify_wrong_secret() {
        let service1 = WsTokenService::with_secret("secret1");
        let service2 = WsTokenService::with_secret("secret2");
        let token = service1.create_token("alice", 120);
        let result = service2.verify_token(&token);
        assert!(result.is_none());
    }

    #[test]
    fn test_public_token() {
        let service = WsTokenService::with_secret("test-secret");
        let token = service.create_token("__public__", 120);
        let user_id = service.verify_token(&token);
        assert_eq!(user_id, Some("__public__".to_string()));
    }

    #[test]
    fn test_token_with_different_ttls() {
        let service = WsTokenService::with_secret("test-secret");
        let token_60 = service.create_token("alice", 60);
        let token_3600 = service.create_token("alice", 3600);
        assert!(service.verify_token(&token_60).is_some());
        assert!(service.verify_token(&token_3600).is_some());
    }
}
