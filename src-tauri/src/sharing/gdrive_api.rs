//! Google Drive REST API helpers for share credential generation and revocation.

use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

/// Errors produced by Google Drive API operations.
#[derive(Debug, Error)]
pub(crate) enum GdriveApiError {
    #[error("Google Drive API request failed")]
    Request(#[from] reqwest::Error),
    #[error("Google Drive API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },
    #[error("token refresh failed: {0}")]
    TokenRefresh(String),
    #[error("folder not found for path prefix: {0}")]
    FolderNotFound(String),
}

/// A refreshed OAuth access token obtained from the token endpoint.
pub(crate) struct GdriveAccessToken {
    /// Short-lived bearer token for Drive API calls.
    pub access_token: String,
}

/// A Drive permission entry created by `gdrive_create_permission`.
pub(crate) struct GdrivePermission {
    /// Opaque permission ID returned by the Drive API (used for later deletion).
    pub permission_id: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct DriveFilesListResponse {
    files: Vec<DriveFile>,
}

#[derive(Deserialize)]
struct DriveFile {
    id: String,
}

#[derive(Deserialize)]
struct DrivePermissionResponse {
    id: String,
}

/// Parses Google Drive OAuth credentials from a rclone.conf INI string for the named remote.
///
/// Returns `Some((client_id, client_secret, refresh_token, root_folder_id))` when the named
/// stanza exists with `type = drive` and all three OAuth fields present.
/// Returns `None` for non-Drive remotes or when the stanza is absent.
pub(crate) fn parse_gdrive_oauth_from_conf(
    conf: &str,
    remote_name: &str,
) -> Option<(String, String, String, Option<String>)> {
    let mut in_target = false;
    let mut is_drive = false;
    let mut client_id: Option<String> = None;
    let mut client_secret: Option<String> = None;
    let mut token_json: Option<String> = None;
    let mut root_folder_id: Option<String> = None;

    for line in conf.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_target
                && is_drive
                && let Some(result) =
                    build_gdrive_result(client_id, client_secret, token_json, root_folder_id)
            {
                return Some(result);
            }
            let name = &trimmed[1..trimmed.len() - 1];
            in_target = name == remote_name;
            is_drive = false;
            client_id = None;
            client_secret = None;
            token_json = None;
            root_folder_id = None;
            continue;
        }
        if in_target && let Some((k, v)) = trimmed.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            match k {
                "type" if v == "drive" => is_drive = true,
                "client_id" => client_id = Some(v.to_owned()),
                "client_secret" => client_secret = Some(v.to_owned()),
                "token" => token_json = Some(v.to_owned()),
                "root_folder_id" if !v.is_empty() => root_folder_id = Some(v.to_owned()),
                _ => {}
            }
        }
    }
    if in_target && is_drive {
        build_gdrive_result(client_id, client_secret, token_json, root_folder_id)
    } else {
        None
    }
}

fn build_gdrive_result(
    client_id: Option<String>,
    client_secret: Option<String>,
    token_json: Option<String>,
    root_folder_id: Option<String>,
) -> Option<(String, String, String, Option<String>)> {
    let cid = client_id.unwrap_or_default();
    let csecret = client_secret.unwrap_or_default();
    let token_str = token_json?;
    let refresh_token = serde_json::from_str::<serde_json::Value>(&token_str)
        .ok()?
        .get("refresh_token")?
        .as_str()?
        .to_owned();
    Some((cid, csecret, refresh_token, root_folder_id))
}

/// Extracts the current `access_token` from a rclone.conf INI string for the named Drive remote.
///
/// Used when no `client_id`/`client_secret` are present (rclone built-in app), so the token
/// cannot be refreshed via the OAuth endpoint. Relies on rclone having written a fresh token
/// during a recent operation.
pub(crate) fn parse_gdrive_access_token_from_conf(conf: &str, remote_name: &str) -> Option<String> {
    let mut in_target = false;
    let mut token_json: Option<String> = None;

    for line in conf.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_target {
                break;
            }
            let name = &trimmed[1..trimmed.len() - 1];
            in_target = name == remote_name;
            continue;
        }
        if in_target
            && let Some((k, v)) = trimmed.split_once('=')
            && k.trim() == "token"
        {
            token_json = Some(v.trim().to_owned());
        }
    }

    let token_str = token_json?;
    serde_json::from_str::<serde_json::Value>(&token_str)
        .ok()?
        .get("access_token")?
        .as_str()?
        .to_owned()
        .into()
}

/// Exchanges a refresh token for a short-lived access token.
///
/// Calls `POST https://oauth2.googleapis.com/token`.
pub(crate) async fn gdrive_refresh_token(
    client: &Client,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<GdriveAccessToken, GdriveApiError> {
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        return Err(GdriveApiError::TokenRefresh(format!(
            "HTTP {status}: {body}"
        )));
    }

    let data: TokenResponse = resp.json().await?;
    Ok(GdriveAccessToken {
        access_token: data.access_token,
    })
}

/// Resolves a rclone-style path prefix (e.g. `shared/<uuid>/`) to its Drive folder ID.
///
/// Walks the path components relative to `root_folder_id` (or `"root"` when absent),
/// querying Drive Files.list at each level.  Retries once after a 2-second delay if
/// the leaf folder is not found on the first attempt (handles Drive propagation lag
/// after a fresh rclone upload).
pub(crate) async fn gdrive_resolve_folder_id(
    client: &Client,
    access_token: &str,
    root_folder_id: Option<&str>,
    path_prefix: &str,
) -> Result<String, GdriveApiError> {
    let components: Vec<&str> = path_prefix.split('/').filter(|s| !s.is_empty()).collect();

    if components.is_empty() {
        return Err(GdriveApiError::FolderNotFound(path_prefix.to_owned()));
    }

    let mut parent_id = root_folder_id.unwrap_or("root").to_owned();

    for (depth, component) in components.iter().enumerate() {
        let is_leaf = depth == components.len() - 1;
        match find_drive_folder(client, access_token, &parent_id, component).await? {
            Some(id) => parent_id = id,
            None if is_leaf => {
                // One retry after short delay for Drive propagation lag.
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                match find_drive_folder(client, access_token, &parent_id, component).await? {
                    Some(id) => parent_id = id,
                    None => {
                        return Err(GdriveApiError::FolderNotFound(format!(
                            "{path_prefix} (component '{component}' not found under parent '{parent_id}')"
                        )));
                    }
                }
            }
            None => {
                return Err(GdriveApiError::FolderNotFound(format!(
                    "{path_prefix} (component '{component}' not found under parent '{parent_id}')"
                )));
            }
        }
    }

    Ok(parent_id)
}

async fn find_drive_folder(
    client: &Client,
    access_token: &str,
    parent_id: &str,
    name: &str,
) -> Result<Option<String>, GdriveApiError> {
    let q = format!(
        "'{parent_id}' in parents and name = '{name}' \
         and mimeType = 'application/vnd.google-apps.folder' and trashed = false"
    );
    let resp = client
        .get("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(access_token)
        .query(&[
            ("q", q.as_str()),
            ("fields", "files(id,name)"),
            ("pageSize", "1"),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        return Err(GdriveApiError::Api { status, body });
    }

    let data: DriveFilesListResponse = resp.json().await?;
    Ok(data.files.into_iter().next().map(|f| f.id))
}

/// Grants a service account read permission on a Drive folder with an optional expiry.
///
/// Calls `POST /drive/v3/files/{folder_id}/permissions`.
/// `expiration_rfc3339` must be an RFC 3339 string with millisecond precision
/// (e.g. `"2025-08-12T12:00:00.000Z"`) or `None` for no expiry.
pub(crate) async fn gdrive_create_permission(
    client: &Client,
    access_token: &str,
    folder_id: &str,
    sa_email: &str,
    expiration_rfc3339: Option<&str>,
) -> Result<GdrivePermission, GdriveApiError> {
    let mut body = serde_json::json!({
        "type": "user",
        "role": "reader",
        "emailAddress": sa_email,
    });
    if let Some(expiry) = expiration_rfc3339 {
        body["expirationTime"] = serde_json::json!(expiry);
    }

    let url = format!("https://www.googleapis.com/drive/v3/files/{folder_id}/permissions");
    let resp = client
        .post(&url)
        .bearer_auth(access_token)
        .query(&[
            ("sendNotificationEmail", "false"),
            ("supportsAllDrives", "true"),
            ("fields", "id"),
        ])
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(400)
            .collect::<String>();
        return Err(GdriveApiError::Api { status, body });
    }

    let data: DrivePermissionResponse = resp.json().await?;
    Ok(GdrivePermission {
        permission_id: data.id,
    })
}

/// Revokes a Drive permission by deleting it.
///
/// Calls `DELETE /drive/v3/files/{folder_id}/permissions/{permission_id}`.
#[allow(dead_code)] // TODO(phase-6): called from revoke_share command
pub(crate) async fn gdrive_delete_permission(
    client: &Client,
    access_token: &str,
    folder_id: &str,
    permission_id: &str,
) -> Result<(), GdriveApiError> {
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{folder_id}/permissions/{permission_id}"
    );
    let resp = client
        .delete(&url)
        .bearer_auth(access_token)
        .query(&[("supportsAllDrives", "true")])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        return Err(GdriveApiError::Api { status, body });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a Drive stanza with all required fields is parsed correctly.
    #[test]
    fn test_parse_gdrive_oauth_from_conf_parses_valid_drive_stanza() {
        let token = r#"{"access_token":"at","token_type":"Bearer","refresh_token":"rt","expiry":"2025-01-01T00:00:00Z"}"#;
        let conf = format!(
            "[gdrive]\ntype = drive\nclient_id = CID\nclient_secret = CSECRET\ntoken = {token}\n"
        );
        let result = parse_gdrive_oauth_from_conf(&conf, "gdrive");
        assert!(result.is_some());
        let (cid, csec, rt, rfi) = result.unwrap();
        assert_eq!(cid, "CID");
        assert_eq!(csec, "CSECRET");
        assert_eq!(rt, "rt");
        assert_eq!(rfi, None);
    }

    /// Verifies that a stanza with root_folder_id is parsed and returned.
    #[test]
    fn test_parse_gdrive_oauth_from_conf_includes_root_folder_id() {
        let token = r#"{"access_token":"at","refresh_token":"rt"}"#;
        let conf = format!(
            "[gdrive]\ntype = drive\nclient_id = CID\nclient_secret = CS\ntoken = {token}\nroot_folder_id = FOLDERID\n"
        );
        let (_, _, _, rfi) = parse_gdrive_oauth_from_conf(&conf, "gdrive").unwrap();
        assert_eq!(rfi, Some("FOLDERID".to_owned()));
    }

    /// Verifies that a non-Drive stanza returns None.
    #[test]
    fn test_parse_gdrive_oauth_from_conf_returns_none_for_b2_stanza() {
        let conf = "[remote]\ntype = b2\naccount = ACC\nkey = KEY\n";
        assert!(parse_gdrive_oauth_from_conf(conf, "remote").is_none());
    }

    /// Verifies that a Drive stanza missing refresh_token returns None.
    #[test]
    fn test_parse_gdrive_oauth_from_conf_returns_none_when_refresh_token_missing() {
        let token = r#"{"access_token":"at"}"#;
        let conf = format!(
            "[gdrive]\ntype = drive\nclient_id = CID\nclient_secret = CS\ntoken = {token}\n"
        );
        assert!(parse_gdrive_oauth_from_conf(&conf, "gdrive").is_none());
    }

    /// Verifies that a multi-remote config returns None for a non-target remote.
    #[test]
    fn test_parse_gdrive_oauth_from_conf_ignores_other_remotes() {
        let token = r#"{"access_token":"at","refresh_token":"rt"}"#;
        let conf = format!(
            "[b2remote]\ntype = b2\naccount = ACC\nkey = KEY\n\
             [gdrive]\ntype = drive\nclient_id = CID\nclient_secret = CS\ntoken = {token}\n"
        );
        assert!(parse_gdrive_oauth_from_conf(&conf, "b2remote").is_none());
        assert!(parse_gdrive_oauth_from_conf(&conf, "gdrive").is_some());
    }
}
