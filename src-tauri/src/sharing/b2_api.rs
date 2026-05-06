//! Backblaze B2 API helpers for scoped application key generation.

use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

/// Errors produced by B2 API operations.
#[derive(Debug, Error)]
pub(crate) enum B2ApiError {
    #[error("B2 API request failed")]
    Request(#[from] reqwest::Error),
    #[error("B2 API error: {0}")]
    Api(String),
}

/// Credentials returned by `b2_authorize_account`.
pub(crate) struct B2Auth {
    /// The B2 account identifier.
    pub account_id: String,
    /// The API URL for subsequent B2 API calls.
    pub api_url: String,
    /// The download URL for reading objects from the bucket.
    pub download_url: String,
    /// The authorization token for subsequent B2 API calls.
    pub authorization_token: String,
}

/// Application key returned by `b2_create_application_key`.
pub(crate) struct B2AppKey {
    /// The identifier for the created application key (safe to log).
    pub application_key_id: String,
    /// The secret application key value (never log).
    pub application_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2AuthorizeResponse {
    account_id: String,
    api_info: B2ApiInfo,
    authorization_token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2ApiInfo {
    storage_api: B2StorageApi,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2StorageApi {
    api_url: String,
    download_url: String,
}

/// Authenticates with B2 using the master key ID and application key.
pub(crate) async fn b2_authorize_account(
    key_id: &str,
    app_key: &str,
) -> Result<B2Auth, B2ApiError> {
    let client = Client::new();
    let resp = client
        .get("https://api.backblazeb2.com/b2api/v3/b2_authorize_account")
        .basic_auth(key_id, Some(app_key))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(B2ApiError::Api("authorization failed".to_owned()));
    }

    let data: B2AuthorizeResponse = resp.json().await?;
    Ok(B2Auth {
        account_id: data.account_id,
        api_url: data.api_info.storage_api.api_url,
        download_url: data.api_info.storage_api.download_url,
        authorization_token: data.authorization_token,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2BucketListResponse {
    buckets: Vec<B2Bucket>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2Bucket {
    bucket_id: String,
    bucket_name: String,
}

/// Returns the bucket ID for a given bucket name.
pub(crate) async fn b2_get_bucket_id(
    client: &Client,
    auth: &B2Auth,
    bucket_name: &str,
) -> Result<String, B2ApiError> {
    let url = format!("{}/b2api/v3/b2_list_buckets", auth.api_url);
    let resp = client
        .post(&url)
        .header("Authorization", &auth.authorization_token)
        .json(&serde_json::json!({
            "accountId": auth.account_id,
            "bucketName": bucket_name,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        let body = body.chars().take(200).collect::<String>();
        return Err(B2ApiError::Api(format!(
            "b2_list_buckets returned HTTP {status}: {body}"
        )));
    }

    let data: B2BucketListResponse = resp.json().await?;
    data.buckets
        .into_iter()
        .find(|b| b.bucket_name == bucket_name)
        .map(|b| b.bucket_id)
        .ok_or_else(|| B2ApiError::Api(format!("bucket '{bucket_name}' not found in account")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2CreateKeyResponse {
    application_key_id: String,
    application_key: String,
}

/// Creates a scoped B2 application key with the given capabilities and prefix.
///
/// `capabilities` should be `["readFiles", "listBuckets"]` at minimum.
/// Add `"writeFiles"` when receipt upload is required.
pub(crate) async fn b2_create_application_key(
    client: &Client,
    auth: &B2Auth,
    bucket_id: &str,
    name_prefix: &str,
    capabilities: &[&str],
    valid_duration_seconds: u32,
) -> Result<B2AppKey, B2ApiError> {
    let url = format!("{}/b2api/v3/b2_create_key", auth.api_url);
    // B2 keyName may only contain letters, digits, and dashes — strip slashes from path prefix.
    let safe_suffix: String = name_prefix
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .take(16)
        .collect();
    let resp = client
        .post(&url)
        .header("Authorization", &auth.authorization_token)
        .json(&serde_json::json!({
            "accountId": auth.account_id,
            "capabilities": capabilities,
            "keyName": format!("arx-share-{}", safe_suffix),
            "validDurationInSeconds": valid_duration_seconds,
            "bucketId": bucket_id,
            "namePrefix": name_prefix,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(B2ApiError::Api(format!(
            "key creation failed ({}): {}",
            status, body
        )));
    }

    let data: B2CreateKeyResponse = resp.json().await?;
    Ok(B2AppKey {
        application_key_id: data.application_key_id,
        application_key: data.application_key,
    })
}

/// Deletes a B2 application key by key ID. Best-effort — errors are logged, not returned.
#[allow(dead_code)] // used in revocation (Step 12)
pub(crate) async fn b2_delete_key(
    client: &Client,
    auth: &B2Auth,
    application_key_id: &str,
) -> Result<(), B2ApiError> {
    let url = format!("{}/b2api/v3/b2_delete_key", auth.api_url);
    let resp = client
        .post(&url)
        .header("Authorization", &auth.authorization_token)
        .json(&serde_json::json!({ "applicationKeyId": application_key_id }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(B2ApiError::Api("key deletion failed".to_owned()));
    }
    Ok(())
}

/// Parses B2 master API key credentials from a rclone.conf INI string.
///
/// Returns `Some((account, key))` for the first stanza with `type = b2`.
/// The bucket is intentionally excluded — it is derived from the rclone remote root path,
/// not the config stanza, because standard rclone B2 remotes do not embed a `bucket` line.
/// Returns `None` if no B2 stanza with both `account` and `key` is found.
pub(crate) fn parse_b2_api_keys_from_conf(conf: &str) -> Option<(String, String)> {
    let mut account: Option<String> = None;
    let mut key: Option<String> = None;
    let mut in_b2_section = false;

    for line in conf.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_b2_section = false;
            account = None;
            key = None;
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            match k {
                "type" if v == "b2" => in_b2_section = true,
                "account" if in_b2_section => account = Some(v.to_owned()),
                "key" if in_b2_section => key = Some(v.to_owned()),
                _ => {}
            }
        }
        if in_b2_section {
            if let (Some(a), Some(k)) = (&account, &key) {
                return Some((a.clone(), k.clone()));
            }
        }
    }
    None
}

/// Parses B2 credentials from a rclone.conf INI string.
///
/// Returns `Some((account, key, bucket))` for the first stanza with `type = b2`.
/// Returns `None` if no B2 stanza is found.
pub(crate) fn parse_b2_credentials_from_conf(conf: &str) -> Option<(String, String, String)> {
    let mut account: Option<String> = None;
    let mut key: Option<String> = None;
    let mut bucket: Option<String> = None;
    let mut in_b2_section = false;

    for line in conf.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_b2_section = false;
            account = None;
            key = None;
            bucket = None;
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            match k {
                "type" if v == "b2" => in_b2_section = true,
                "account" if in_b2_section => account = Some(v.to_owned()),
                "key" if in_b2_section => key = Some(v.to_owned()),
                "bucket" if in_b2_section => bucket = Some(v.to_owned()),
                _ => {}
            }
        }
        if in_b2_section {
            if let (Some(a), Some(k), Some(b)) = (&account, &key, &bucket) {
                return Some((a.clone(), k.clone(), b.clone()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a B2 section with only account and key is parsed by `parse_b2_api_keys_from_conf`.
    #[test]
    fn test_parse_b2_api_keys_from_conf_parses_account_and_key_without_bucket() {
        let conf = "[remote]\ntype = b2\naccount = ACC123\nkey = SECRETKEY\n";
        assert_eq!(
            parse_b2_api_keys_from_conf(conf),
            Some(("ACC123".to_owned(), "SECRETKEY".to_owned()))
        );
    }

    /// Verifies that `parse_b2_api_keys_from_conf` also works when bucket is present.
    #[test]
    fn test_parse_b2_api_keys_from_conf_parses_when_bucket_present() {
        let conf = "[remote]\ntype = b2\naccount = ACC123\nkey = SECRETKEY\nbucket = my-bucket\n";
        assert_eq!(
            parse_b2_api_keys_from_conf(conf),
            Some(("ACC123".to_owned(), "SECRETKEY".to_owned()))
        );
    }

    /// Verifies that `parse_b2_api_keys_from_conf` returns None for a non-B2 section.
    #[test]
    fn test_parse_b2_api_keys_from_conf_returns_none_for_non_b2_section() {
        let conf = "[remote]\ntype = s3\naccount = ACC123\nkey = SECRETKEY\n";
        assert_eq!(parse_b2_api_keys_from_conf(conf), None);
    }

    /// Verifies that a well-formed B2 section is parsed correctly.
    #[test]
    fn test_parse_b2_credentials_from_conf_parses_valid_b2_section() {
        let conf = "[remote]\ntype = b2\naccount = ACC123\nkey = SECRETKEY\nbucket = my-bucket\n";
        let result = parse_b2_credentials_from_conf(conf);
        assert_eq!(
            result,
            Some((
                "ACC123".to_owned(),
                "SECRETKEY".to_owned(),
                "my-bucket".to_owned()
            ))
        );
    }

    /// Verifies that a non-B2 section returns `None`.
    #[test]
    fn test_parse_b2_credentials_from_conf_returns_none_for_non_b2_section() {
        let conf = "[remote]\ntype = s3\naccount = ACC123\nkey = SECRETKEY\nbucket = my-bucket\n";
        let result = parse_b2_credentials_from_conf(conf);
        assert_eq!(result, None);
    }

    /// Verifies that an empty config returns `None`.
    #[test]
    fn test_parse_b2_credentials_from_conf_returns_none_for_empty_conf() {
        let result = parse_b2_credentials_from_conf("");
        assert_eq!(result, None);
    }

    /// Verifies that a B2 section missing `bucket` returns `None`.
    #[test]
    fn test_parse_b2_credentials_from_conf_returns_none_when_bucket_missing() {
        let conf = "[remote]\ntype = b2\naccount = ACC123\nkey = SECRETKEY\n";
        let result = parse_b2_credentials_from_conf(conf);
        assert_eq!(result, None);
    }
}
