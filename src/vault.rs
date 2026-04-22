//! Vault file-browser components: breadcrumbs, file list, drag-drop zone, and
//! the upload button.
//!
//! All components read vault state via `use_vault()` and trigger navigation via
//! `use_vault_actions()` from the `VaultProvider` context.

use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::components::{Button, Modal, Spinner};
use crate::dialog::{open_file_dialog, open_save_dialog};
use crate::drag_drop::on_file_drop;
use crate::invoke::invoke_command;
use crate::ipc_channel::IpcChannel;
use crate::ipc_types::{
    DeleteFileRequest, DownloadFileRequest, FileEntry, GetFileContentRequest, ProgressUpdate,
    UploadFileRequest,
};
use crate::state::{use_vault, use_vault_actions};
use crate::transfer::ProgressModal;

// ─── Preview type detection ──────────────────────────────────────────────────

/// Maximum file size for preview: 50 MiB (52,428,800 bytes).
const MAX_PREVIEW_SIZE_BYTES: u64 = 52_428_800;

/// Detects if a file's size allows preview (≤ 50 MiB).
pub fn file_size_allows_preview(size_bytes: u64) -> bool {
    size_bytes <= MAX_PREVIEW_SIZE_BYTES
}

/// Returns the file extension in lowercase, or empty string if no extension found.
fn get_file_extension(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default()
}

/// Detects if a file extension is previewable (MVP: 6 text/image types).
pub fn extension_is_previewable(filename: &str) -> bool {
    matches!(
        get_file_extension(filename).as_str(),
        "txt" | "md" | "log" | "csv" | "png" | "jpg" | "jpeg" | "gif" | "webp"
    )
}

/// Detects if a previewable file is an image type.
fn is_image_type(filename: &str) -> bool {
    matches!(
        get_file_extension(filename).as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp"
    )
}

// ─── Path helper ─────────────────────────────────────────────────────────────

/// Splits a vault-relative path into `(label, cumulative_path)` pairs.
///
/// The root segment is always rendered as `("Vault", "/")`.
/// Trailing slashes are stripped before splitting.
pub fn split_path_segments(path: &str) -> Vec<(String, String)> {
    let trimmed = path.trim_start_matches('/').trim_end_matches('/');
    let mut out = vec![("Vault".into(), "/".into())];
    if trimmed.is_empty() {
        return out;
    }
    let mut acc = String::new();
    for seg in trimmed.split('/') {
        acc.push('/');
        acc.push_str(seg);
        out.push((seg.to_string(), acc.clone()));
    }
    out
}

/// Joins a vault-relative `current_path` and an `entry_name` into a full vault path.
///
/// Handles the root special case (`current_path == "/"`) to avoid double slashes.
/// Trailing slashes on `current_path` are stripped before joining.
pub fn join_vault_path(current_path: &str, entry_name: &str) -> String {
    if current_path == "/" {
        format!("/{entry_name}")
    } else {
        format!("{}/{entry_name}", current_path.trim_end_matches('/'))
    }
}

// ─── Base64 decoding ─────────────────────────────────────────────────────────

/// Decodes a base64 string to bytes using JavaScript's atob.
fn base64_decode(encoded: &str) -> Option<Vec<u8>> {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = atob)]
        fn atob_js(s: &str) -> String;
    }

    let decoded_str = atob_js(encoded);
    Some(decoded_str.into_bytes())
}

// ─── ContentViewerModal ──────────────────────────────────────────────────────

/// Modal component for viewing file content inline (text or image).
///
/// Implements Zero-Trace: clears signal state on dismiss (not just hidden).
#[component]
pub fn ContentViewerModal(
    /// RwSignal holding the file content bytes (base64-encoded from backend).
    content: RwSignal<Option<String>>,
    /// Filename for display and type detection.
    filename: String,
) -> impl IntoView {
    let is_open = Signal::derive(move || content.get().is_some());
    let is_image = is_image_type(&filename);

    view! {
        <Modal
            open=is_open
            on_close=move || content.set(None)
        >
            <div class="w-full max-w-3xl max-h-[80vh] flex flex-col">
                <div class="flex items-center justify-between mb-4">
                    <h2 class="text-lg text-bone truncate">{filename.clone()}</h2>
                    <button
                        class="text-text-muted hover:text-bone"
                        on:click=move |_| content.set(None)
                    >
                        "✕"
                    </button>
                </div>
                <Show
                    when=move || is_image
                    fallback=move || {
                        view! {
                            <div class="flex-1 overflow-auto bg-surface-overlay rounded p-4">
                                <pre class="text-text-secondary text-sm font-mono whitespace-pre-wrap break-words">
                                    {move || {
                                        content.get().and_then(|encoded| {
                                            let decoded = base64_decode(&encoded)?;
                                            String::from_utf8(decoded).ok()
                                        })
                                    }}
                                </pre>
                            </div>
                        }
                    }
                >
                    <div class="flex-1 flex items-center justify-center overflow-auto bg-surface-overlay rounded">
                        {move || {
                            let encoded = content.get()?;
                            let img_src = format!("data:image/png;base64,{}", &encoded);
                            Some(view! {
                                <img
                                    src=img_src
                                    alt="File preview"
                                    class="max-w-full max-h-full object-contain"
                                />
                            })
                        }}
                    </div>
                </Show>
            </div>
        </Modal>
    }
}

// ─── Breadcrumbs ─────────────────────────────────────────────────────────────

/// Navigation breadcrumb bar showing the current vault path.
///
/// Each segment is a clickable button that navigates to the corresponding
/// directory.
#[component]
pub fn Breadcrumbs(
    /// Current vault-relative path signal.
    #[prop(into)]
    path: Signal<String>,
) -> impl IntoView {
    let actions = use_vault_actions();
    view! {
        <nav class="flex gap-1 text-sm text-bone" aria-label="Vault navigation">
            <For
                each=move || split_path_segments(&path.get())
                key=|(_, full)| full.clone()
                children=move |(label, full)| {
                    let full_click = full.clone();
                    view! {
                        <button
                            class="hover:text-rune"
                            on:click=move |_| actions.navigate(full_click.clone())
                        >
                            {label}
                        </button>
                        <span class="text-text-muted">"/"</span>
                    }
                }
            />
        </nav>
    }
}

// ─── FileItem ────────────────────────────────────────────────────────────────

/// Single row in the file list with download, delete, and preview actions.
///
/// Directories are rendered with a folder icon and navigate on click.
/// Files are rendered with a file icon and support download/delete/preview actions.
#[component]
pub fn FileItem(
    /// The file or directory entry to display.
    entry: FileEntry,
) -> impl IntoView {
    let vault = use_vault();
    let actions = use_vault_actions();
    let is_dir = entry.entry_type == "directory";

    let entry_clone = entry.clone();
    let (show_delete_confirm, set_show_delete_confirm) = signal(false);
    let file_content = RwSignal::new(None::<String>);
    let (download_progress_channel, set_download_progress_channel) =
        signal::<Option<IpcChannel<ProgressUpdate>>>(None);
    let (delete_error, set_delete_error) = signal::<Option<String>>(None);

    let file_name = entry.name.clone();
    let can_preview = !is_dir
        && file_size_allows_preview(entry.size_bytes)
        && extension_is_previewable(&entry.name);
    
    let entry_stored = StoredValue::new(entry_clone.clone());
    let file_name_stored = StoredValue::new(file_name.clone());

    view! {
        <>
            <div class="flex items-center gap-3 p-2 rounded hover:bg-surface-overlay">
                <span class="text-rune w-4 text-center">
                    {if is_dir { "📁" } else { "📄" }}
                </span>
                <span
                    class=move || {
                        format!(
                            "flex-1 text-bone text-sm {}",
                            if !is_dir && can_preview {
                                "cursor-pointer hover:underline"
                            } else {
                                ""
                            }
                        )
                    }
                    on:click=move |_| {
                        if !is_dir
                            && file_size_allows_preview(entry_clone.size_bytes)
                            && extension_is_previewable(&entry_clone.name)
                        {
                            let entry = entry_clone.clone();
                            leptos::task::spawn_local(async move {
                                let req = GetFileContentRequest {
                                    file_id: entry.id.clone(),
                                };
                                match invoke_command::<GetFileContentRequest, String>("get_file_content", &req)
                                    .await
                                {
                                    Ok(content) => file_content.set(Some(content)),
                                    Err(err) => {
                                        leptos::logging::error!("Failed to fetch file content: {}", err.message);
                                    }
                                }
                            });
                        }
                    }
                >
                    {entry.name.clone()}
                </span>
                <span class="text-text-muted text-xs">
                    {if is_dir {
                        String::new()
                    } else {
                        format!("{} B", entry.size_bytes)
                    }}
                </span>
                <Show
                    when=move || !is_dir
                    fallback=|| ()
                >
                    <div class="flex gap-1">
                        <button
                            class="text-text-muted hover:text-rune text-sm px-2 py-1"
                            title="Download"
                            on:click=move |_| {
                                let entry = entry_stored.get_value();
                                let actions = actions;
                                let set_download_progress_channel = set_download_progress_channel;
                                leptos::task::spawn_local(async move {
                                    let default_name = entry.name.clone();
                                    if let Some(dest_path) = open_save_dialog(Some(&default_name)).await {
                                        let channel = IpcChannel::<ProgressUpdate>::new();
                                        set_download_progress_channel.set(Some(channel.clone()));

                                        let req = DownloadFileRequest {
                                            file_id: entry.id.clone(),
                                            destination_path: dest_path,
                                            progress: channel.inner().clone(),
                                        };

                                        match invoke_command::<DownloadFileRequest, ()>("download_file", &req).await {
                                            Ok(()) => {}
                                            Err(err) => actions.set_error(err.message),
                                        }
                                    }
                                });
                            }
                        >
                            "⬇"
                        </button>
                        <button
                            class="text-text-muted hover:text-danger text-sm px-2 py-1"
                            title="Delete"
                            on:click=move |_| {
                                set_show_delete_confirm.set(true);
                                set_delete_error.set(None);
                            }
                        >
                            "🗑"
                        </button>
                    </div>
                </Show>
            </div>

            // Delete confirmation modal
            <Show
                when=move || show_delete_confirm.get()
                fallback=|| ()
            >
                <Modal
                    open=Signal::derive(move || show_delete_confirm.get())
                    on_close=move || set_show_delete_confirm.set(false)
                >
                    <div class="w-80">
                        <h2 class="text-lg text-bone mb-4">
                            "Delete " {file_name_stored.get_value()} "?"
                        </h2>
                        <p class="text-text-secondary text-sm mb-6">
                            "This action cannot be undone."
                        </p>
                        {move || {
                            delete_error.get().map(|err| view! {
                                <p class="text-danger text-sm mb-4">{err}</p>
                            })
                        }}
                        <div class="flex gap-2 justify-end">
                            <button
                                class="px-4 py-2 rounded bg-surface-overlay hover:bg-surface-overlay-hover text-bone"
                                on:click=move |_| set_show_delete_confirm.set(false)
                            >
                                "Cancel"
                            </button>
                            <button
                                class="px-4 py-2 rounded bg-danger hover:bg-danger-hover text-white"
                                on:click=move |_| {
                                    let entry = entry_stored.get_value();
                                    let current_path = vault.get_untracked().current_path;
                                    let actions = actions;
                                    let set_show_delete_confirm = set_show_delete_confirm;
                                    let set_delete_error = set_delete_error;

                                    leptos::task::spawn_local(async move {
                                        let req = DeleteFileRequest {
                                            file_id: entry.id.clone(),
                                        };

                                        match invoke_command::<DeleteFileRequest, ()>("delete_file", &req).await {
                                            Ok(()) => {
                                                set_show_delete_confirm.set(false);
                                                actions.navigate(current_path);
                                            }
                                            Err(err) => set_delete_error.set(Some(err.message)),
                                        }
                                    });
                                }
                            >
                                "Delete"
                            </button>
                        </div>
                    </div>
                </Modal>
            </Show>

            // Download progress modal
            <Show
                when=move || download_progress_channel.get().is_some()
                fallback=|| ()
            >
                {move || {
                    download_progress_channel.get().map(|channel| {
                        view! {
                            <ProgressModal
                                channel=channel
                                title="Downloading file"
                                on_close=move || set_download_progress_channel.set(None)
                            />
                        }
                    })
                }}
            </Show>

            // File content viewer modal
            <ContentViewerModal
                content=file_content
                filename=entry_stored.get_value().name.clone()
            />
        </>
    }
}

// ─── FileList ────────────────────────────────────────────────────────────────

/// Scrollable list of vault entries.
#[component]
pub fn FileList(
    /// The list of file and directory entries to display.
    entries: Signal<Vec<FileEntry>>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col divide-y divide-border-subtle">
            <For
                each=move || entries.get()
                key=|e| e.id.clone()
                children=move |entry| view! { <FileItem entry=entry /> }
            />
        </div>
    }
}

// ─── DropZone ────────────────────────────────────────────────────────────────

/// File drop zone that accepts dragged files and initiates upload.
///
/// Subscribes to `onDragDropEvent` on mount and unsubscribes in `on_cleanup`.
/// Uses `UploadFileRequest` per file; progress channel is wired in Phase 6.5.
#[component]
pub fn DropZone(children: Children) -> impl IntoView {
    let vault = use_vault();
    let vault_actions = use_vault_actions();

    Effect::new(move |_| {
        let vault_actions = vault_actions;
        let unsub = on_file_drop(move |paths| {
            let current_path = vault.get_untracked().current_path;
            for source_path in paths {
                let file_name = source_path.split(['/', '\\']).next_back().unwrap_or("file");
                let vault_path = join_vault_path(&current_path, file_name);
                let upload_path = current_path.clone();
                let req = UploadFileRequest {
                    source_path: source_path.clone(),
                    vault_path,
                    progress: JsValue::undefined(),
                };
                let va = vault_actions;
                leptos::task::spawn_local(async move {
                    match invoke_command::<UploadFileRequest, ()>("upload_file", &req).await {
                        Ok(()) => va.navigate(upload_path),
                        Err(err) => va.set_error(err.message),
                    }
                });
            }
        });
        on_cleanup(unsub);
    });

    view! {
        <div class="relative w-full h-full">
            {children()}
        </div>
    }
}

// ─── UploadButton ────────────────────────────────────────────────────────────

/// Upload button that opens a native file picker and uploads the selected file.
#[component]
pub fn UploadButton() -> impl IntoView {
    let vault = use_vault();
    let vault_actions = use_vault_actions();
    let (loading, set_loading) = signal(false);

    let on_click = move |_| {
        let vault_actions = vault_actions;
        let set_loading = set_loading;
        leptos::task::spawn_local(async move {
            let Some(source_path) = open_file_dialog().await else {
                return;
            };
            let current_path = vault.get_untracked().current_path;
            let file_name = source_path.split(['/', '\\']).next_back().unwrap_or("file");
            let vault_path = join_vault_path(&current_path, file_name);
            set_loading.set(true);
            let req = UploadFileRequest {
                source_path,
                vault_path,
                progress: JsValue::undefined(),
            };
            match invoke_command::<UploadFileRequest, ()>("upload_file", &req).await {
                Ok(()) => vault_actions.navigate(current_path),
                Err(err) => vault_actions.set_error(err.message),
            }
            set_loading.set(false);
        });
    };

    view! {
        <Button loading=loading on_click=on_click>"Upload File"</Button>
    }
}

// ─── VaultBrowser ────────────────────────────────────────────────────────────

/// Main vault file browser view.
///
/// Navigates to the root directory on mount and displays a breadcrumb trail,
/// file list, and upload controls. Wrapped in a `DropZone` to support
/// drag-drop uploads.
#[component]
pub fn VaultBrowser() -> impl IntoView {
    let vault = use_vault();
    let vault_actions = use_vault_actions();

    Effect::new(move |_| {
        vault_actions.navigate("/".into());
    });

    let current_path = Signal::derive(move || vault.read().current_path.clone());

    view! {
        <div class="flex flex-col gap-4 h-full">
            <div class="flex items-center justify-between">
                <Breadcrumbs path=current_path />
                <UploadButton />
            </div>

            {move || vault.read().error.clone().map(|e| view! {
                <p class="text-danger text-sm">{e}</p>
            })}

            <Show when=move || vault.read().loading>
                <div class="flex justify-center p-8">
                    <Spinner size="h-8 w-8" />
                </div>
            </Show>

            <DropZone>
                <FileList entries=Signal::derive(move || vault.read().files.clone()) />
            </DropZone>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_vault_path_root_prepends_slash() {
        assert_eq!(join_vault_path("/", "file.txt"), "/file.txt");
    }

    #[test]
    fn test_join_vault_path_nested_dir_appends_segment() {
        assert_eq!(join_vault_path("/docs", "reports"), "/docs/reports");
    }

    #[test]
    fn test_join_vault_path_trailing_slash_stripped() {
        assert_eq!(join_vault_path("/docs/", "report.pdf"), "/docs/report.pdf");
    }

    #[test]
    fn test_split_path_segments_root_returns_single_root_segment() {
        let segs = split_path_segments("/");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0], ("Vault".to_string(), "/".to_string()));
    }

    #[test]
    fn test_split_path_segments_nested_path_splits_into_ordered_pairs() {
        let segs = split_path_segments("/docs/reports");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], ("Vault".to_string(), "/".to_string()));
        assert_eq!(segs[1], ("docs".to_string(), "/docs".to_string()));
        assert_eq!(
            segs[2],
            ("reports".to_string(), "/docs/reports".to_string())
        );
    }

    #[test]
    fn test_split_path_segments_strips_trailing_slash() {
        let segs = split_path_segments("/docs/");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1].0, "docs");
        assert_eq!(segs[1].1, "/docs");
    }

    #[test]
    fn test_file_size_allows_preview_returns_true_at_limit() {
        assert!(file_size_allows_preview(MAX_PREVIEW_SIZE_BYTES));
    }

    #[test]
    fn test_file_size_allows_preview_returns_false_above_limit() {
        assert!(!file_size_allows_preview(MAX_PREVIEW_SIZE_BYTES + 1));
    }

    #[test]
    fn test_extension_is_previewable_text_types() {
        assert!(extension_is_previewable("file.txt"));
        assert!(extension_is_previewable("README.md"));
        assert!(extension_is_previewable("debug.log"));
        assert!(extension_is_previewable("data.csv"));
    }

    #[test]
    fn test_extension_is_previewable_image_types() {
        assert!(extension_is_previewable("photo.png"));
        assert!(extension_is_previewable("image.jpg"));
        assert!(extension_is_previewable("picture.jpeg"));
        assert!(extension_is_previewable("animation.gif"));
        assert!(extension_is_previewable("vector.webp"));
    }

    #[test]
    fn test_extension_is_previewable_returns_false_for_unsupported() {
        assert!(!extension_is_previewable("archive.zip"));
        assert!(!extension_is_previewable("document.pdf"));
        assert!(!extension_is_previewable("video.mp4"));
    }

    #[test]
    fn test_extension_is_previewable_case_insensitive() {
        assert!(extension_is_previewable("FILE.TXT"));
        assert!(extension_is_previewable("Photo.PNG"));
        assert!(extension_is_previewable("readme.MD"));
    }
}
