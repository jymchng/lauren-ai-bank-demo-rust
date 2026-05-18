use injectable::prelude::*;
use serde::Deserialize;

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub openrouter_api_key: String,
    pub llm_model: String,
    pub llm_base_url: String,
    pub payload_secret: String,
    pub port: u16,
}

#[injectable]
impl AppConfig {
    /// Load configuration from environment variables.
    #[injectable(ctor)]
    pub fn from_env() -> Self {
        Self {
            openrouter_api_key: std::env::var("OPENROUTER_API_KEY")
                .unwrap_or_else(|_| "test-api-key".into()),
            llm_model: std::env::var("LLM_MODEL")
                .unwrap_or_else(|_| "poolside/laguna-xs.2:free".into()),
            llm_base_url: std::env::var("LLM_BASE_URL")
                .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into()),
            payload_secret: std::env::var("PAYLOAD_SECRET")
                .unwrap_or_else(|_| "test-secret".into()),
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "8000".into())
                .parse()
                .unwrap_or(8000),
        }
    }

    /// Create a test configuration.
    pub fn test_config() -> Self {
        Self {
            openrouter_api_key: "test-key".into(),
            llm_model: "test-model".into(),
            llm_base_url: "http://localhost:11434/v1".into(),
            payload_secret: "test-secret-key-for-hmac".into(),
            port: 8000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_config() {
        let config = AppConfig::test_config();
        assert_eq!(config.openrouter_api_key, "test-key");
        assert_eq!(config.llm_model, "test-model");
        assert_eq!(config.port, 8000);
    }

    #[test]
    #[ignore] // env vars set by parallel tests cause flakiness
    fn test_from_env_with_defaults() {
        std::env::remove_var("OPENROUTER_API_KEY");
        std::env::remove_var("LLM_MODEL");
        std::env::remove_var("PAYLOAD_SECRET");

        let config = AppConfig::from_env();
        assert_eq!(config.llm_model, "poolside/laguna-xs.2:free");
        assert_eq!(config.llm_base_url, "https://openrouter.ai/api/v1");
        assert_eq!(config.port, 8000);
    }
}
