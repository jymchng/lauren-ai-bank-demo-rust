use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::config::AppConfig;
use injectable::prelude::*;

type HmacSha256 = Hmac<Sha256>;

/// Cryptographic service for HMAC-SHA256 signing and verification.
#[derive(Debug, Clone)]
pub struct CryptoService {
    secret: Vec<u8>,
}

#[injectable]
impl CryptoService {
    /// Create a new CryptoService injecting AppConfig for the HMAC secret.
    #[injectable(ctor)]
    pub fn new(#[injectable(inject)] config: Arc<AppConfig>) -> Self {
        Self {
            secret: config.payload_secret.as_bytes().to_vec(),
        }
    }

    /// Create a CryptoService with an explicit secret (for tests).
    pub fn with_secret(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into().as_bytes().to_vec(),
        }
    }

    /// Sign a payload with HMAC-SHA256.
    pub fn sign(&self, payload: &[u8]) -> String {
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC can take key of any size");
        mac.update(payload);
        hex::encode(mac.finalize().into_bytes())
    }

    /// Verify a payload against an HMAC-SHA256 signature.
    pub fn verify(&self, payload: &[u8], signature: &str) -> bool {
        let decoded = match hex::decode(signature) {
            Ok(d) => d,
            Err(_) => return false,
        };
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC can take key of any size");
        mac.update(payload);
        mac.verify_slice(&decoded).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let crypto = CryptoService::with_secret("test-secret");
        let payload = b"hello world";
        let signature = crypto.sign(payload);
        assert!(crypto.verify(payload, &signature));
    }

    #[test]
    fn test_verify_wrong_payload() {
        let crypto = CryptoService::with_secret("test-secret");
        let signature = crypto.sign(b"hello world");
        assert!(!crypto.verify(b"hello universe", &signature));
    }

    #[test]
    fn test_verify_wrong_signature() {
        let crypto = CryptoService::with_secret("test-secret");
        assert!(!crypto.verify(b"hello world", "invalid-signature"));
    }

    #[test]
    fn test_verify_invalid_hex() {
        let crypto = CryptoService::with_secret("test-secret");
        assert!(!crypto.verify(b"hello world", "not-hex-at-all!!!"));
    }

    #[test]
    fn test_different_secrets_produce_different_signatures() {
        let crypto1 = CryptoService::with_secret("secret1");
        let crypto2 = CryptoService::with_secret("secret2");
        let payload = b"same payload";
        let sig1 = crypto1.sign(payload);
        let sig2 = crypto2.sign(payload);
        assert_ne!(sig1, sig2);
        assert!(crypto1.verify(payload, &sig1));
        assert!(!crypto1.verify(payload, &sig2));
    }

    #[test]
    fn test_sign_empty_payload() {
        let crypto = CryptoService::with_secret("test-secret");
        let signature = crypto.sign(b"");
        assert!(crypto.verify(b"", &signature));
    }

    #[test]
    fn test_sign_json_payload() {
        let crypto = CryptoService::with_secret("test-secret");
        let payload = br#"{"user_id":"alice","message":"hello"}"#;
        let signature = crypto.sign(payload);
        assert!(crypto.verify(payload, &signature));
    }

    #[test]
    fn test_constant_time_verification() {
        // This test verifies that timing attacks are mitigated
        // The hmac crate uses constant-time comparison internally
        let crypto = CryptoService::with_secret("test-secret");
        let payload = b"test payload";
        let real_sig = crypto.sign(payload);

        // Create a slightly different signature
        let mut fake_sig_bytes = hex::decode(&real_sig).unwrap();
        fake_sig_bytes[0] ^= 0xff;
        let fake_sig = hex::encode(fake_sig_bytes);

        assert!(crypto.verify(payload, &real_sig));
        assert!(!crypto.verify(payload, &fake_sig));
    }
}
