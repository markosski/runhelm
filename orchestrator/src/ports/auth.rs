use async_trait::async_trait;

pub const DEFAULT_NAMESPACE_ID: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    pub namespace_id: String,
}

#[async_trait]
pub trait AuthPort {
    async fn authenticate_api_token(&self, token: &str) -> anyhow::Result<Option<AuthContext>>;
}
