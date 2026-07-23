use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION, request::Parts},
};
use std::ops::Deref;

use crate::core::namespace::{Namespace, NamespaceResolverPort};

use super::router::PublicAppState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestNamespace(Namespace);

impl From<Namespace> for RequestNamespace {
    fn from(namespace: Namespace) -> Self {
        Self(namespace)
    }
}

impl Deref for RequestNamespace {
    type Target = Namespace;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<PublicAppState> for RequestNamespace {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &PublicAppState,
    ) -> Result<Self, Self::Rejection> {
        resolve_request_namespace(&parts.headers, state.namespace_resolver.as_ref())
            .await
            .map(Self)
    }
}

async fn resolve_request_namespace(
    headers: &HeaderMap,
    resolver: &(dyn NamespaceResolverPort + Send + Sync),
) -> Result<Namespace, StatusCode> {
    resolver
        .resolve(bearer_credential(headers).ok())
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

fn bearer_credential(headers: &HeaderMap) -> Result<&str, StatusCode> {
    let mut values = headers.get_all(AUTHORIZATION).iter();
    let value = values.next().ok_or(StatusCode::UNAUTHORIZED)?;
    if values.next().is_some() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let value = value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
    let (scheme, credential) = value.split_once(' ').ok_or(StatusCode::UNAUTHORIZED)?;

    if !scheme.eq_ignore_ascii_case("Bearer") || !is_bearer_token(credential) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(credential)
}

fn is_bearer_token(value: &str) -> bool {
    let mut padding_started = false;
    let mut has_token_character = false;

    for character in value.chars() {
        if character == '=' {
            padding_started = true;
        } else if !padding_started
            && (character.is_ascii_alphanumeric()
                || matches!(character, '-' | '.' | '_' | '~' | '+' | '/'))
        {
            has_token_character = true;
        } else {
            return false;
        }
    }

    has_token_character
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    const NAMESPACE: &str = "550e8400-e29b-41d4-a716-446655440000";

    struct RecordingResolver {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl NamespaceResolverPort for RecordingResolver {
        async fn resolve(&self, api_key: Option<&str>) -> anyhow::Result<Namespace> {
            let api_key =
                api_key.ok_or_else(|| anyhow::anyhow!("bearer credential is required"))?;
            assert_eq!(api_key, "api-key");
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(Namespace::new(NAMESPACE).unwrap())
        }
    }

    struct DefaultResolver {
        namespace: Namespace,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl NamespaceResolverPort for DefaultResolver {
        async fn resolve(&self, _api_key: Option<&str>) -> anyhow::Result<Namespace> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.namespace.clone())
        }
    }

    #[tokio::test]
    async fn configured_default_takes_precedence_without_inspecting_authorization() {
        let resolver = Arc::new(DefaultResolver {
            namespace: Namespace::new(NAMESPACE).unwrap(),
            calls: AtomicUsize::new(0),
        });
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "not-even-a-bearer-value".parse().unwrap());

        let namespace = resolve_request_namespace(&headers, resolver.as_ref())
            .await
            .unwrap();

        assert_eq!(namespace.as_str(), NAMESPACE);
        assert_eq!(resolver.calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn valid_bearer_credential_reaches_resolver_without_default() {
        let resolver = Arc::new(RecordingResolver {
            calls: AtomicUsize::new(0),
        });
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer api-key".parse().unwrap());

        let namespace = resolve_request_namespace(&headers, resolver.as_ref())
            .await
            .unwrap();

        assert_eq!(namespace.as_str(), NAMESPACE);
        assert_eq!(resolver.calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn missing_or_malformed_bearer_credentials_are_unauthorized() {
        let resolver = Arc::new(RecordingResolver {
            calls: AtomicUsize::new(0),
        });
        for value in [
            None,
            Some(""),
            Some("Bearer"),
            Some("Bearer "),
            Some("Bearer  api-key"),
            Some("Basic api-key"),
            Some("Bearer api key"),
            Some("Bearer api:key"),
            Some("Bearer ="),
            Some("Bearer api=key"),
        ] {
            let mut headers = HeaderMap::new();
            if let Some(value) = value {
                headers.insert(AUTHORIZATION, value.parse().unwrap());
            }

            assert_eq!(
                resolve_request_namespace(&headers, resolver.as_ref()).await,
                Err(StatusCode::UNAUTHORIZED),
                "{value:?}"
            );
        }

        assert_eq!(resolver.calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn multiple_authorization_headers_are_unauthorized() {
        let resolver = Arc::new(RecordingResolver {
            calls: AtomicUsize::new(0),
        });
        let mut headers = HeaderMap::new();
        headers.append(AUTHORIZATION, "Bearer api-key".parse().unwrap());
        headers.append(AUTHORIZATION, "Bearer second-key".parse().unwrap());

        assert_eq!(
            resolve_request_namespace(&headers, resolver.as_ref()).await,
            Err(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(resolver.calls.load(Ordering::Relaxed), 0);
    }
}
