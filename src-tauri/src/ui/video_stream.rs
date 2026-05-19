//! Video streaming via a registered `arxvault://` URI scheme protocol.
//!
//! The WebView `<video>` element issues HTTP range requests against
//! `arxvault://localhost/view/{file_id}`.  Each request decrypts only the chunks
//! that overlap with the requested byte range — no decrypted data is written to disk
//! (Zero-Trace) and at most one chunk's plaintext occupies RAM at any time.

use tauri::Manager as _;
use uuid::Uuid;

use crate::storage::MetadataStore as _;
use crate::storage::cloud::sync::fetch_missing_file_blobs;
use crate::storage::vault_ops::download_file_range_to_memory;
use crate::ui::commands_common::require_active_session;
use crate::ui::file_commands::extract_kek;
use crate::ui::state::AppState;
use crate::ui::validation::validate_file_id;
use crate::ui::vault_paths::vault_staging_dir;

/// Maximum bytes returned per open-ended range request.
///
/// Caps responses for `Range: bytes=N-` to two 4 MiB chunks so the entire file is
/// never decrypted in a single request.  The browser will issue follow-up requests.
const MAX_RANGE_BYTES: u64 = 8 * 1024 * 1024;

// ─── Builder registration ─────────────────────────────────────────────────────

/// Registers the `arxvault://` custom URI scheme on the Tauri builder.
///
/// Must be called before `Builder::run`.  The scheme handler is async: each WebView
/// range request spawns a Tokio task that decrypts the requested byte range and
/// responds with `206 Partial Content`.
pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol("arxvault", handle_request)
}

// ─── IPC command ─────────────────────────────────────────────────────────────

/// Returns the platform-appropriate base URL for the `arxvault://` scheme.
///
/// Tauri maps custom schemes to `http://{scheme}.localhost` on Windows and
/// `{scheme}://localhost` on macOS and Linux.  The frontend uses this URL as the
/// `src` attribute of the `<video>` element.
#[tauri::command]
pub fn video_scheme_base_url() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "http://arxvault.localhost"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "arxvault://localhost"
    }
}

// ─── Scheme handler ───────────────────────────────────────────────────────────

fn handle_request<R: tauri::Runtime>(
    ctx: tauri::UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let response = serve_video_range(&app, &request).await;
        responder.respond(response);
    });
}

async fn serve_video_range<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let state = app.state::<AppState>();

    if require_active_session(&state).await.is_err() {
        return plain_error(tauri::http::StatusCode::FORBIDDEN, "Vault is locked");
    }

    // Extract file_id from path `/view/{file_id}`.
    let path = request.uri().path();
    let file_id_str = match path.strip_prefix("/view/") {
        Some(id) => id.trim_end_matches('/'),
        None => return plain_error(tauri::http::StatusCode::NOT_FOUND, "Not found"),
    };
    if validate_file_id(file_id_str).is_err() {
        return plain_error(tauri::http::StatusCode::BAD_REQUEST, "Invalid file ID");
    }
    let node_uuid = match Uuid::parse_str(file_id_str) {
        Ok(id) => id,
        Err(_) => return plain_error(tauri::http::StatusCode::BAD_REQUEST, "Invalid file ID"),
    };

    let vault_id = match state.session_manager.active_vault_id().await {
        Some(id) => id,
        None => return plain_error(tauri::http::StatusCode::FORBIDDEN, "No active vault"),
    };
    let staging_dir = vault_staging_dir(&vault_id);

    let kek = match extract_kek(&state).await {
        Ok(k) => k,
        Err(_) => return plain_error(tauri::http::StatusCode::INTERNAL_SERVER_ERROR, "Key error"),
    };

    let db_store = match state.session_manager.get_metadata_store().await {
        Some(s) => s,
        None => return plain_error(tauri::http::StatusCode::FORBIDDEN, "Vault locked"),
    };
    let db = &*db_store;

    let node = match db.get_node(node_uuid).await {
        Ok(n) => n,
        Err(_) => return plain_error(tauri::http::StatusCode::NOT_FOUND, "File not found"),
    };
    let file_size = node.size_bytes;
    if file_size == 0 {
        return plain_error(tauri::http::StatusCode::NO_CONTENT, "Empty file");
    }

    // Fetch any cloud-only blobs that are missing from local staging.
    let cloud = state.cloud_transport.read().await.clone();
    if fetch_missing_file_blobs(node_uuid, db, &staging_dir, cloud.as_ref(), None)
        .await
        .is_err()
    {
        return plain_error(
            tauri::http::StatusCode::SERVICE_UNAVAILABLE,
            "Cloud fetch failed",
        );
    }

    let (range_start, range_end, is_range_request) = match parse_range_header(request, file_size) {
        Ok(r) => r,
        Err(()) => {
            return plain_error(
                tauri::http::StatusCode::RANGE_NOT_SATISFIABLE,
                "Invalid range",
            );
        }
    };

    let bytes = match download_file_range_to_memory(
        node_uuid,
        db,
        &kek,
        &staging_dir,
        range_start,
        range_end,
    )
    .await
    {
        Ok(b) => b,
        Err(_) => {
            return plain_error(
                tauri::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Decryption failed",
            );
        }
    };

    let content_length = range_end - range_start + 1;
    let mime = mime_from_name(&node.name);

    let mut builder = tauri::http::Response::builder()
        .header(tauri::http::header::CONTENT_TYPE, mime)
        .header(tauri::http::header::ACCEPT_RANGES, "bytes")
        .header(tauri::http::header::CONTENT_LENGTH, content_length);

    if is_range_request {
        builder = builder
            .status(tauri::http::StatusCode::PARTIAL_CONTENT)
            .header(
                tauri::http::header::CONTENT_RANGE,
                format!("bytes {range_start}-{range_end}/{file_size}"),
            );
    } else {
        builder = builder.status(tauri::http::StatusCode::OK);
    }

    // `bytes` (Zeroizing<Vec<u8>>) zeroes on drop at end of scope. The Vec<u8> copy
    // passed to builder.body() is Tauri-owned after the move; post-handoff zeroization
    // is not possible with this API. Accepted limitation.
    builder.body(bytes.to_vec()).unwrap_or_else(|_| {
        plain_error(
            tauri::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Build error",
        )
    })
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Parses an HTTP `Range` header into `(range_start, range_end, is_range_request)`.
///
/// Open-ended ranges (`bytes=N-`) are capped at [`MAX_RANGE_BYTES`] from the start
/// so the entire file is never decrypted in one shot.  The returned `range_end` is
/// always a valid, inclusive byte offset within `[0, file_size - 1]`.
fn parse_range_header(
    request: &tauri::http::Request<Vec<u8>>,
    file_size: u64,
) -> Result<(u64, u64, bool), ()> {
    let last_byte = file_size - 1;
    let Some(val) = request.headers().get(tauri::http::header::RANGE) else {
        return Ok((0, last_byte, false));
    };
    let val = val.to_str().map_err(|_| ())?;
    let val = val.strip_prefix("bytes=").ok_or(())?;
    let (start_str, end_str) = val.split_once('-').ok_or(())?;
    let start: u64 = start_str.parse().map_err(|_| ())?;
    if start >= file_size {
        return Err(());
    }
    let end = if end_str.is_empty() {
        (start + MAX_RANGE_BYTES - 1).min(last_byte)
    } else {
        let requested: u64 = end_str.parse().map_err(|_| ())?;
        if requested < start {
            return Err(());
        }
        requested.min(last_byte)
    };
    Ok((start, end, true))
}

/// Returns a video MIME type inferred from the file extension, falling back to
/// `video/octet-stream` for unrecognised containers.
fn mime_from_name(filename: &str) -> &'static str {
    match filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        _ => "video/octet-stream",
    }
}

/// Builds a plain-text error response.
fn plain_error(status: tauri::http::StatusCode, body: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header(tauri::http::header::CONTENT_TYPE, "text/plain")
        .body(body.as_bytes().to_vec())
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(range_header: Option<&str>) -> tauri::http::Request<Vec<u8>> {
        let mut builder = tauri::http::Request::builder().uri("arxvault://localhost/view/test");
        if let Some(val) = range_header {
            builder = builder.header(tauri::http::header::RANGE, val);
        }
        builder.body(vec![]).expect("request should build")
    }

    // ─── parse_range_header ───────────────────────────────────────────────────

    /// Absent Range header returns full-file range with is_range_request=false.
    #[test]
    fn test_parse_range_header_absent_returns_full_file_range() {
        let req = make_request(None);
        let (start, end, is_range) = parse_range_header(&req, 1000).expect("should parse");
        assert_eq!(start, 0);
        assert_eq!(end, 999);
        assert!(!is_range);
    }

    /// Bounded range returns exact start/end with is_range_request=true.
    #[test]
    fn test_parse_range_header_bounded_range_returns_exact_bounds() {
        let req = make_request(Some("bytes=100-199"));
        let (start, end, is_range) = parse_range_header(&req, 1000).expect("should parse");
        assert_eq!(start, 100);
        assert_eq!(end, 199);
        assert!(is_range);
    }

    /// Open-ended range is capped at MAX_RANGE_BYTES from start.
    #[test]
    fn test_parse_range_header_open_ended_caps_at_max_range_bytes() {
        let req = make_request(Some("bytes=0-"));
        let file_size = MAX_RANGE_BYTES * 4;
        let (start, end, is_range) = parse_range_header(&req, file_size).expect("should parse");
        assert_eq!(start, 0);
        assert_eq!(end, MAX_RANGE_BYTES - 1);
        assert!(is_range);
    }

    /// Open-ended range does not exceed file_size - 1.
    #[test]
    fn test_parse_range_header_open_ended_clamps_to_file_end() {
        let req = make_request(Some("bytes=0-"));
        let file_size = 100u64;
        let (start, end, is_range) = parse_range_header(&req, file_size).expect("should parse");
        assert_eq!(start, 0);
        assert_eq!(end, 99);
        assert!(is_range);
    }

    /// Bounded range end beyond EOF is clamped to last byte.
    #[test]
    fn test_parse_range_header_end_beyond_eof_clamps_to_last_byte() {
        let req = make_request(Some("bytes=0-9999"));
        let (start, end, _) = parse_range_header(&req, 100).expect("should parse");
        assert_eq!(start, 0);
        assert_eq!(end, 99);
    }

    /// Start equal to file_size returns error.
    #[test]
    fn test_parse_range_header_start_at_file_size_returns_error() {
        let req = make_request(Some("bytes=1000-1999"));
        assert!(parse_range_header(&req, 1000).is_err());
    }

    /// Inverted range (end < start) returns error.
    #[test]
    fn test_parse_range_header_inverted_range_returns_error() {
        let req = make_request(Some("bytes=500-100"));
        assert!(parse_range_header(&req, 1000).is_err());
    }

    /// Missing "bytes=" prefix returns error.
    #[test]
    fn test_parse_range_header_missing_prefix_returns_error() {
        let req = make_request(Some("0-100"));
        assert!(parse_range_header(&req, 1000).is_err());
    }

    // ─── mime_from_name ───────────────────────────────────────────────────────

    /// Known video extensions map to the correct MIME types.
    #[test]
    fn test_mime_from_name_known_extensions_return_correct_mime() {
        assert_eq!(mime_from_name("clip.mp4"), "video/mp4");
        assert_eq!(mime_from_name("clip.m4v"), "video/mp4");
        assert_eq!(mime_from_name("clip.mov"), "video/quicktime");
        assert_eq!(mime_from_name("clip.webm"), "video/webm");
        assert_eq!(mime_from_name("clip.avi"), "video/x-msvideo");
        assert_eq!(mime_from_name("clip.mkv"), "video/x-matroska");
    }

    /// Unknown or absent extension falls back to octet-stream.
    #[test]
    fn test_mime_from_name_unknown_extension_returns_octet_stream() {
        assert_eq!(mime_from_name("clip.wmv"), "video/octet-stream");
        assert_eq!(mime_from_name("noextension"), "video/octet-stream");
    }

    /// Extension matching is case-insensitive.
    #[test]
    fn test_mime_from_name_uppercase_extension_matches() {
        assert_eq!(mime_from_name("clip.MP4"), "video/mp4");
        assert_eq!(mime_from_name("clip.MOV"), "video/quicktime");
    }
}
