use async_trait::async_trait;
use std::collections::HashMap;

use crate::ports::auth::{AuthContext, AuthPort};

#[derive(Debug, Clone)]
pub struct EnvAuthAdapter {
    tokens: HashMap<String, String>,
}

impl EnvAuthAdapter {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_config(std::env::var("RUNHELM_API_TOKENS").unwrap_or_default())
    }

    pub fn from_config(config: impl AsRef<str>) -> anyhow::Result<Self> {
        let mut tokens = HashMap::new();

        for entry in config.as_ref().split(',').map(str::trim) {
            if entry.is_empty() {
                continue;
            }

            let Some((token, namespace_id)) = entry.split_once('=') else {
                anyhow::bail!("RUNHELM_API_TOKENS entries must use token=namespace format");
            };

            let token = token.trim();
            let namespace_id = namespace_id.trim();
            if token.is_empty() || namespace_id.is_empty() {
                anyhow::bail!("RUNHELM_API_TOKENS entries cannot contain empty token or namespace");
            }

            tokens.insert(token.to_string(), namespace_id.to_string());
        }

        Ok(Self { tokens })
    }
}

#[async_trait]
impl AuthPort for EnvAuthAdapter {
    async fn authenticate_api_token(&self, token: &str) -> anyhow::Result<Option<AuthContext>> {
        Ok(self.tokens.get(token).map(|namespace_id| AuthContext {
            namespace_id: namespace_id.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn authenticates_configured_tokens() {
        let auth =
            EnvAuthAdapter::from_config("token-a=namespace-a, token-b = namespace-b").unwrap();

        assert_eq!(
            auth.authenticate_api_token("token-a").await.unwrap(),
            Some(AuthContext {
                namespace_id: "namespace-a".to_string(),
            })
        );
        assert_eq!(
            auth.authenticate_api_token("token-b").await.unwrap(),
            Some(AuthContext {
                namespace_id: "namespace-b".to_string(),
            })
        );
        assert_eq!(auth.authenticate_api_token("missing").await.unwrap(), None);
    }

    #[test]
    fn rejects_malformed_config_entries() {
        assert!(EnvAuthAdapter::from_config("token-without-namespace").is_err());
        assert!(EnvAuthAdapter::from_config("=namespace").is_err());
        assert!(EnvAuthAdapter::from_config("token=").is_err());
    }
}
