//! Cloud connection descriptor from the cloud-sync design "Connection Descriptor" section.

use serde::{Deserialize, Serialize};

use super::CloudTransportError;
use super::remote_path::validate_remote_root_component;

/// Endpoint metadata describing where cloud transport operations are rooted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudEndpoint {
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub path_prefix: String,
}

impl CloudEndpoint {
    /// Validates endpoint values before persistence and use.
    pub fn validate(&self) -> Result<(), CloudTransportError> {
        if self.provider.trim().is_empty() {
            return Err(CloudTransportError::Other(
                "cloud endpoint rejected: provider is required".to_owned(),
            ));
        }
        if self.bucket.chars().any(char::is_control) {
            return Err(CloudTransportError::Other(
                "cloud endpoint rejected: bucket contains control characters".to_owned(),
            ));
        }
        validate_remote_root_component("bucket", &self.bucket, true)?;
        if self.region.chars().any(char::is_control) {
            return Err(CloudTransportError::Other(
                "cloud endpoint rejected: region contains control characters".to_owned(),
            ));
        }
        validate_remote_root_component("path_prefix", &self.path_prefix, true)?;
        validate_endpoint_url(&self.endpoint)
    }
}

fn validate_endpoint_url(endpoint: &str) -> Result<(), CloudTransportError> {
    validate_endpoint_url_with_policy(endpoint, local_dev_http_override_enabled())
}

fn validate_endpoint_url_with_policy(
    endpoint: &str,
    local_dev_http_override_enabled: bool,
) -> Result<(), CloudTransportError> {
    if endpoint.is_empty() {
        return Ok(());
    }
    if endpoint.chars().any(char::is_control) {
        return Err(CloudTransportError::Other(
            "cloud endpoint rejected: endpoint contains control characters".to_owned(),
        ));
    }
    if endpoint.contains('?') || endpoint.contains('#') {
        return Err(CloudTransportError::Other(
            "cloud endpoint rejected: query and fragment are not allowed".to_owned(),
        ));
    }

    let (scheme, rest) = endpoint
        .strip_prefix("https://")
        .map(|rest| ("https", rest))
        .or_else(|| endpoint.strip_prefix("http://").map(|rest| ("http", rest)))
        .ok_or_else(|| {
            CloudTransportError::Other(
                "cloud endpoint rejected: endpoint must start with https:// (or local-dev http:// with explicit opt-in)".to_owned(),
            )
        })?;

    if rest.is_empty() {
        return Err(CloudTransportError::Other(
            "cloud endpoint rejected: endpoint host is required".to_owned(),
        ));
    }

    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return Err(CloudTransportError::Other(
            "cloud endpoint rejected: endpoint host is required".to_owned(),
        ));
    }
    if authority.contains('@') {
        return Err(CloudTransportError::Other(
            "cloud endpoint rejected: userinfo is not allowed in endpoint".to_owned(),
        ));
    }
    let authority_host = extract_authority_host(authority)?;
    if authority_host.is_empty() {
        return Err(CloudTransportError::Other(
            "cloud endpoint rejected: endpoint host is required".to_owned(),
        ));
    }
    if scheme == "http"
        && !(local_dev_http_override_enabled && is_local_development_host(authority_host))
    {
        return Err(CloudTransportError::Other(
            "cloud endpoint rejected: http:// is disabled by default; only local-development endpoints are allowed when ARX_RUNA_ALLOW_LOCAL_DEV_HTTP_ENDPOINTS=1".to_owned(),
        ));
    }

    let path = &rest[authority_end..];
    if !path.is_empty() && path != "/" {
        return Err(CloudTransportError::Other(
            "cloud endpoint rejected: endpoint path is not allowed".to_owned(),
        ));
    }

    Ok(())
}

fn extract_authority_host(authority: &str) -> Result<&str, CloudTransportError> {
    if authority.starts_with('[') {
        let bracket_end = authority.find(']').ok_or_else(|| {
            CloudTransportError::Other("cloud endpoint rejected: invalid IPv6 host".to_owned())
        })?;
        return Ok(&authority[1..bracket_end]);
    }
    Ok(authority
        .split_once(':')
        .map_or(authority, |(host, _)| host))
}

fn is_local_development_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.")
}

fn local_dev_http_override_enabled() -> bool {
    std::env::var("ARX_RUNA_ALLOW_LOCAL_DEV_HTTP_ENDPOINTS")
        .map(|value| {
            let normalised = value.trim().to_ascii_lowercase();
            matches!(normalised.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{CloudEndpoint, validate_endpoint_url_with_policy};
    use crate::storage::cloud::CloudTransportError;

    fn valid_endpoint() -> CloudEndpoint {
        CloudEndpoint {
            provider: "s3".to_owned(),
            bucket: "bucket".to_owned(),
            region: "us-east-1".to_owned(),
            endpoint: "https://s3.example.com".to_owned(),
            path_prefix: "vault".to_owned(),
        }
    }

    #[test]
    fn test_validate_accepts_basic_https_endpoint() {
        assert!(valid_endpoint().validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_userinfo_in_endpoint() {
        let mut endpoint = valid_endpoint();
        endpoint.endpoint = "https://user:pass@s3.example.com".to_owned();
        assert!(matches!(
            endpoint.validate(),
            Err(CloudTransportError::Other(_))
        ));
    }

    #[test]
    fn test_validate_rejects_query_and_fragment() {
        let mut endpoint = valid_endpoint();
        endpoint.endpoint = "https://s3.example.com?token=secret".to_owned();
        assert!(matches!(
            endpoint.validate(),
            Err(CloudTransportError::Other(_))
        ));

        endpoint.endpoint = "https://s3.example.com/#fragment".to_owned();
        assert!(matches!(
            endpoint.validate(),
            Err(CloudTransportError::Other(_))
        ));
    }

    #[test]
    fn test_validate_rejects_non_root_path() {
        let mut endpoint = valid_endpoint();
        endpoint.endpoint = "https://s3.example.com/custom/path".to_owned();
        assert!(matches!(
            endpoint.validate(),
            Err(CloudTransportError::Other(_))
        ));
    }

    #[test]
    fn test_validate_rejects_http_endpoint_by_default() {
        let result = validate_endpoint_url_with_policy("http://localhost:9000", false);
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_allows_local_http_when_explicitly_opted_in() {
        let result = validate_endpoint_url_with_policy("http://127.0.0.1:9000", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rejects_non_local_http_even_with_opt_in() {
        let result = validate_endpoint_url_with_policy("http://s3.example.com", true);
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_rejects_path_prefix_with_leading_slash_for_root_consistency() {
        let mut endpoint = valid_endpoint();
        endpoint.path_prefix = "/vault".to_owned();
        assert!(matches!(
            endpoint.validate(),
            Err(CloudTransportError::Other(_))
        ));
    }

    #[test]
    fn test_validate_rejects_bucket_parent_traversal_for_root_consistency() {
        let mut endpoint = valid_endpoint();
        endpoint.bucket = "bucket/../escape".to_owned();
        assert!(matches!(
            endpoint.validate(),
            Err(CloudTransportError::Other(_))
        ));
    }
}
