//! Vault file-browser components: breadcrumbs, file list, drag-drop zone, and
//! the upload button.
//!
//! All components read vault state via `use_vault()` and trigger navigation via
//! `use_vault_actions()` from the `VaultProvider` context.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::{Button, Modal, Spinner};
use crate::dialog::{open_file_dialog, open_save_dialog};
use crate::drag_drop::on_file_drop;
use crate::invoke::{invoke_command, invoke_command_with_channel};
use crate::ipc_channel::IpcChannel;
use crate::ipc_types::{
    ComposeEmailWithAttachmentRequest, CreateVaultDirectoryRequest, DeleteDirectoryRequest,
    DeleteFileRequest, DestinationEntry, DownloadFileRequest, FileContentResponse, FileEntry,
    GetFileContentRequest, ListLocalDirectoryRequest, LocalEntry, PrefetchVideoRequest,
    ProgressUpdate, RevealInExplorerRequest, ShareResponse, StatLocalPathRequest,
    UploadFileRequest,
};
use crate::shares::ShareModal;
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

/// Detects if a file extension is previewable (text, image, or video types).
pub fn extension_is_previewable(filename: &str) -> bool {
    matches!(
        get_file_extension(filename).as_str(),
        "txt"
            | "md"
            | "log"
            | "csv"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "mp4"
            | "m4v"
            | "mov"
            | "webm"
            | "avi"
            | "mkv"
    )
}

/// Detects if a previewable file is an image type.
fn is_image_type(filename: &str) -> bool {
    matches!(
        get_file_extension(filename).as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp"
    )
}

/// Detects if a file is a video type supported by the arxvault:// stream handler.
fn is_video_type(filename: &str) -> bool {
    matches!(
        get_file_extension(filename).as_str(),
        "mp4" | "m4v" | "mov" | "webm" | "avi" | "mkv"
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
    /// RwSignal holding the decoded file content from the backend.
    content: RwSignal<Option<FileContentResponse>>,
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
                        class="text-text-muted hover:text-bone cursor-pointer"
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
                                        content.get().and_then(|fc| {
                                            let decoded = base64_decode(&fc.data_base64)?;
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
                            let fc = content.get()?;
                            let img_src = format!("data:{};base64,{}", fc.mime_type, fc.data_base64);
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

// ─── VideoViewerModal ─────────────────────────────────────────────────────────

/// Modal component for streaming video from the `arxvault://` custom URI scheme.
///
/// Zero-Trace: the scheme handler decrypts only the requested byte range in RAM;
/// no decrypted bytes are written to disk.  Clearing `video_url` dismisses the modal.
#[component]
pub fn VideoViewerModal(
    /// RwSignal holding the full `arxvault://` (or `http://arxvault.localhost`) URL.
    video_url: RwSignal<Option<String>>,
    /// Filename shown in the modal header.
    filename: String,
) -> impl IntoView {
    let is_open = Signal::derive(move || video_url.get().is_some());
    view! {
        <Modal
            open=is_open
            on_close=move || video_url.set(None)
        >
            <div class="w-full max-w-4xl max-h-[85vh] flex flex-col">
                <div class="flex items-center justify-between mb-4">
                    <h2 class="text-lg text-bone truncate">{filename.clone()}</h2>
                    <button
                        class="text-text-muted hover:text-bone cursor-pointer"
                        on:click=move |_| video_url.set(None)
                    >
                        "✕"
                    </button>
                </div>
                <div class="flex-1 flex items-center justify-center bg-surface-overlay rounded overflow-hidden">
                    {move || {
                        video_url.get().map(|url| view! {
                            <video
                                src=url
                                controls=true
                                autoplay=true
                                class="max-w-full max-h-full"
                            />
                        })
                    }}
                </div>
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
                            class="hover:text-rune cursor-pointer"
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
    /// Whether the primary destination supports file sharing.
    ///
    /// When `false`, the share button is disabled with an explanatory tooltip.
    sharing_supported: Signal<bool>,
) -> impl IntoView {
    let vault = use_vault();
    let actions = use_vault_actions();
    let is_dir = entry.entry_type == "directory";
    let is_pending_flush = entry.pending_flush;

    let entry_clone = entry.clone();
    let (show_delete_confirm, set_show_delete_confirm) = signal(false);
    let (show_download_warn, set_show_download_warn) = signal(false);
    let (show_share_modal, set_show_share_modal) = signal(false);
    let share_result = RwSignal::new(None::<ShareResponse>);
    let file_content = RwSignal::new(None::<FileContentResponse>);
    let video_url = RwSignal::new(None::<String>);
    let (preview_progress_channel, set_preview_progress_channel) =
        signal::<Option<IpcChannel<ProgressUpdate>>>(None);
    let (download_progress_channel, set_download_progress_channel) =
        signal::<Option<IpcChannel<ProgressUpdate>>>(None);
    let (delete_error, set_delete_error) = signal::<Option<String>>(None);

    let file_name = entry.name.clone();
    let is_video = is_video_type(&entry.name);
    // Videos stream via arxvault:// scheme — no size limit; other types cap at 50 MiB.
    let can_preview = !is_dir
        && !is_pending_flush
        && extension_is_previewable(&entry.name)
        && (is_video || file_size_allows_preview(entry.size_bytes));

    let entry_stored = StoredValue::new(entry_clone.clone());
    let file_name_stored = StoredValue::new(file_name.clone());
    let file_id_stored = StoredValue::new(entry.id.clone());

    view! {
        <>
            <div class="flex items-center gap-4 px-3 py-3 rounded hover:bg-surface-overlay">
                <span class="text-rune w-6 text-xl text-center">
                    {if is_dir { "📁" } else { "📄" }}
                </span>
                <span
                    class=move || {
                        format!(
                            "flex-1 text-bone text-sm {}",
                            if is_dir || can_preview {
                                "cursor-pointer hover:underline"
                            } else {
                                ""
                            }
                        )
                    }
                    on:click=move |_| {
                        if is_dir {
                            actions.navigate(entry_clone.id.clone());
                            return;
                        }
                        if !extension_is_previewable(&entry_clone.name) {
                            return;
                        }
                        if is_video {
                            let file_id = entry_clone.id.clone();
                            let channel = IpcChannel::<ProgressUpdate>::new();
                            set_preview_progress_channel.set(Some(channel.clone()));
                            spawn_local(async move {
                                let req = PrefetchVideoRequest {
                                    file_id: file_id.clone(),
                                };
                                match invoke_command_with_channel::<
                                    PrefetchVideoRequest,
                                    String,
                                >(
                                    "prefetch_video",
                                    &req,
                                    "progress",
                                    channel.inner(),
                                )
                                .await
                                {
                                    Ok(base_url) => {
                                        set_preview_progress_channel.set(None);
                                        video_url.set(Some(format!(
                                            "{base_url}/view/{file_id}"
                                        )));
                                    }
                                    Err(err) => {
                                        set_preview_progress_channel.set(None);
                                        leptos::logging::error!(
                                            "Failed to prefetch video: {}",
                                            err.message
                                        );
                                    }
                                }
                            });
                        } else if file_size_allows_preview(entry_clone.size_bytes) {
                            let entry = entry_clone.clone();
                            let channel = IpcChannel::<ProgressUpdate>::new();
                            set_preview_progress_channel.set(Some(channel.clone()));
                            spawn_local(async move {
                                let req = GetFileContentRequest {
                                    file_id: entry.id.clone(),
                                };
                                match invoke_command_with_channel::<
                                    GetFileContentRequest,
                                    FileContentResponse,
                                >(
                                    "get_file_content",
                                    &req,
                                    "progress",
                                    channel.inner(),
                                )
                                .await
                                {
                                    Ok(content) => {
                                        set_preview_progress_channel.set(None);
                                        file_content.set(Some(content));
                                    }
                                    Err(err) => {
                                        set_preview_progress_channel.set(None);
                                        leptos::logging::error!(
                                            "Failed to fetch file content: {}",
                                            err.message
                                        );
                                    }
                                }
                            });
                        }
                    }
                >
                    {entry.name.clone()}
                    <Show
                        when=move || is_pending_flush
                        fallback=|| ()
                    >
                        <span
                            class="ml-1 text-xs text-amber-400"
                            title="File is queued for packing. Press Sync to upload."
                        >
                            "Encrypting…"
                        </span>
                    </Show>
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
                    <div class="flex gap-2 items-center">
                        <button
                            class=move || {
                                if is_pending_flush {
                                    "text-text-muted text-xl px-2 py-1 cursor-not-allowed opacity-50"
                                } else {
                                    "text-text-muted hover:text-rune cursor-pointer text-xl px-2 py-1 transition-transform hover:scale-125"
                                }
                            }
                            title=move || {
                                if is_pending_flush {
                                    "File is queued for packing. Press Sync to upload."
                                } else {
                                    "Download"
                                }
                            }
                            prop:disabled=is_pending_flush
                            on:click=move |_| {
                                if is_pending_flush {
                                    return;
                                }
                                set_show_download_warn.set(true);
                            }
                        >
                            "⬇"
                        </button>
                        {move || {
                            if sharing_supported.get() {
                                view! {
                                    <button
                                        class="text-text-muted hover:text-rune cursor-pointer text-xl px-2 py-1 transition-transform hover:scale-125"
                                        title="Share"
                                        on:click=move |_| {
                                            set_show_share_modal.set(true);
                                            share_result.set(None);
                                        }
                                    >
                                        "↗"
                                    </button>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <button
                                        class="text-text-muted/30 text-xl px-2 py-1 cursor-not-allowed"
                                        title="Sharing requires Backblaze B2 or Google Drive as primary destination"
                                        disabled=true
                                    >
                                        "↗"
                                    </button>
                                }
                                .into_any()
                            }
                        }}
                        <button
                            class="text-text-muted hover:text-danger cursor-pointer text-xl px-2 py-1 transition-transform hover:scale-125"
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
                <Show
                    when=move || is_dir
                    fallback=|| ()
                >
                    <div class="flex gap-2 items-center">
                        <button
                            class="text-text-muted hover:text-danger cursor-pointer text-xl px-2 py-1 transition-transform hover:scale-125"
                            title="Delete folder"
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
                            {if is_dir {
                                "Delete folder and all its contents?".to_owned()
                            } else {
                                format!("Delete {}?", file_name_stored.get_value())
                            }}
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
                                class="px-4 py-2 rounded bg-surface-overlay hover:bg-steel cursor-pointer text-bone transition-colors"
                                on:click=move |_| set_show_delete_confirm.set(false)
                            >
                                "Cancel"
                            </button>
                            <button
                                class="px-4 py-2 rounded bg-danger hover:bg-danger/80 cursor-pointer text-white transition-colors"
                                on:click=move |_| {
                                    let entry = entry_stored.get_value();
                                    let current_path = vault.get_untracked().current_path;
                                    let actions = actions;
                                    let set_show_delete_confirm = set_show_delete_confirm;
                                    let set_delete_error = set_delete_error;

                                    leptos::task::spawn_local(async move {
                                        if is_dir {
                                            let req = DeleteDirectoryRequest {
                                                directory_id: entry.id.clone(),
                                            };
                                            match invoke_command::<DeleteDirectoryRequest, ()>(
                                                "delete_directory",
                                                &req,
                                            )
                                            .await
                                            {
                                                Ok(()) => {
                                                    set_show_delete_confirm.set(false);
                                                    actions.navigate(current_path);
                                                }
                                                Err(err) => {
                                                    set_delete_error.set(Some(err.message));
                                                }
                                            }
                                        } else {
                                            let req = DeleteFileRequest {
                                                file_id: entry.id.clone(),
                                            };
                                            match invoke_command::<DeleteFileRequest, ()>(
                                                "delete_file",
                                                &req,
                                            )
                                            .await
                                            {
                                                Ok(()) => {
                                                    set_show_delete_confirm.set(false);
                                                    actions.navigate(current_path);
                                                }
                                                Err(err) => {
                                                    set_delete_error.set(Some(err.message));
                                                }
                                            }
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

            // Download export warning modal
            <Show
                when=move || show_download_warn.get()
                fallback=|| ()
            >
                <Modal
                    open=Signal::derive(move || show_download_warn.get())
                    on_close=move || set_show_download_warn.set(false)
                >
                    <div class="w-96">
                        <h2 class="text-lg text-bone mb-4">"Export unencrypted file?"</h2>
                        <p class="text-text-secondary text-sm mb-6">
                            "The exported file will be written to disk in plaintext, outside vault protection. You are responsible for the exported copy."
                        </p>
                        <div class="flex gap-2 justify-end">
                            <button
                                class="px-4 py-2 rounded bg-surface-overlay hover:bg-steel cursor-pointer text-bone transition-colors"
                                on:click=move |_| set_show_download_warn.set(false)
                            >
                                "Cancel"
                            </button>
                            <button
                                class="px-4 py-2 rounded bg-amber-600 hover:bg-amber-500 cursor-pointer text-white transition-colors"
                                on:click=move |_| {
                                    set_show_download_warn.set(false);
                                    let entry = entry_stored.get_value();
                                    let actions = actions;
                                    let set_download_progress_channel = set_download_progress_channel;
                                    leptos::task::spawn_local(async move {
                                        let default_name = entry.name.clone();
                                        if let Some(dest_path) =
                                            open_save_dialog(Some(&default_name)).await
                                        {
                                            let channel = IpcChannel::<ProgressUpdate>::new();
                                            set_download_progress_channel.set(Some(channel.clone()));
                                            let req = DownloadFileRequest {
                                                file_id: entry.id.clone(),
                                                destination_path: dest_path,
                                            };
                                            match invoke_command_with_channel::<
                                                DownloadFileRequest,
                                                (),
                                            >(
                                                "download_file",
                                                &req,
                                                "progress",
                                                channel.inner(),
                                            )
                                            .await
                                            {
                                                Ok(()) => {}
                                                Err(err) => actions.set_error(err.message),
                                            }
                                        }
                                    });
                                }
                            >
                                "Export Anyway"
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

            // Preview loading progress modal
            <Show
                when=move || preview_progress_channel.get().is_some()
                fallback=|| ()
            >
                {move || {
                    preview_progress_channel.get().map(|channel| {
                        view! {
                            <ProgressModal
                                channel=channel
                                title="Loading file"
                                on_close=move || set_preview_progress_channel.set(None)
                            />
                        }
                    })
                }}
            </Show>

            // File content viewer modal (text / image)
            <ContentViewerModal
                content=file_content
                filename=entry_stored.get_value().name.clone()
            />

            // Video viewer modal (streams via arxvault:// scheme — Zero-Trace)
            <VideoViewerModal
                video_url=video_url
                filename=entry_stored.get_value().name.clone()
            />

            // Share modal (files only)
            <Show
                when=move || show_share_modal.get() && !is_dir
                fallback=|| ()
            >
                <ShareModal
                    file_id=file_id_stored.get_value()
                    file_name=file_name_stored.get_value()
                    on_close=move || set_show_share_modal.set(false)
                    on_success=Callback::new(move |response: ShareResponse| {
                        set_show_share_modal.set(false);
                        share_result.set(Some(response));
                    })
                />
            </Show>

            // Share result panel
            {move || share_result.get().map(|result| {
                let package_path = result.package_path.clone();
                let file_name = std::path::Path::new(&package_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&package_path)
                    .to_owned();
                let contact_email = result.contact_email.clone();
                let file_name_hint = file_name.clone();
                // Use the contact's email if available; empty string opens the
                // mail client with a blank To field so the user can fill it in.
                let email_for_button = contact_email.clone().unwrap_or_default();
                let path_for_email = package_path.clone();

                view! {
                    <div class="p-4 bg-stone border border-steel rounded mb-4">
                        <p class="text-bone font-semibold mb-2">"Share package ready"</p>
                        <p class="text-text-secondary text-sm mb-3">{file_name}</p>
                        <div class="flex gap-2 flex-wrap">
                            <button
                                class="px-3 py-1 text-sm text-bone bg-steel rounded cursor-pointer hover:bg-rune/20 transition-colors"
                                on:click={
                                    let path = package_path.clone();
                                    move |_| {
                                        let path = path.clone();
                                        spawn_local(async move {
                                            let _ = invoke_command::<RevealInExplorerRequest, ()>(
                                                "reveal_in_explorer",
                                                &RevealInExplorerRequest { path },
                                            ).await;
                                        });
                                    }
                                }
                            >
                                "Reveal in Explorer"
                            </button>
                            <button
                                class="px-3 py-1 text-sm text-bone bg-rune rounded cursor-pointer hover:bg-rune/80 transition-colors"
                                on:click=move |_| {
                                    let req = ComposeEmailWithAttachmentRequest {
                                        package_path: path_for_email.clone(),
                                        recipient_email: email_for_button.clone(),
                                    };
                                    spawn_local(async move {
                                        let _ = invoke_command::<ComposeEmailWithAttachmentRequest, ()>(
                                            "compose_email_with_attachment",
                                            &req,
                                        ).await;
                                    });
                                }
                            >
                                "Compose email"
                            </button>
                            <button
                                class="px-3 py-1 text-sm text-text-secondary cursor-pointer hover:text-bone transition-colors"
                                on:click=move |_| share_result.set(None)
                            >
                                "Close"
                            </button>
                        </div>
                        {contact_email.map(|_| view! {
                            <p class="text-text-secondary text-xs mt-3">
                                "Remember to attach "
                                <span class="text-bone font-mono">{file_name_hint}</span>
                                " to the email before sending."
                            </p>
                        })}
                    </div>
                }
            })}
        </>
    }
}

// ─── FileList ────────────────────────────────────────────────────────────────

/// Scrollable list of vault entries.
#[component]
pub fn FileList(
    /// The list of file and directory entries to display.
    entries: Signal<Vec<FileEntry>>,
    /// Whether the primary destination supports file sharing.
    sharing_supported: Signal<bool>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col divide-y divide-border-subtle" data-testid="file-list">
            <For
                each=move || entries.get()
                key=|e| e.id.clone()
                children=move |entry| view! { <FileItem entry=entry sharing_supported=sharing_supported /> }
            />
        </div>
    }
}

// ─── DropZone ────────────────────────────────────────────────────────────────

/// Uploads a single file to the vault and streams progress via `set_upload_channel`.
///
/// Returns `Ok(())` on success or the error message on failure.
async fn upload_one_file(
    source_path: String,
    vault_path: String,
    set_upload_channel: WriteSignal<Option<IpcChannel<ProgressUpdate>>>,
) -> Result<(), String> {
    let req = UploadFileRequest {
        source_path,
        vault_path,
    };
    let channel = IpcChannel::<ProgressUpdate>::new();
    set_upload_channel.set(Some(channel.clone()));
    invoke_command_with_channel::<UploadFileRequest, FileEntry>(
        "upload_file",
        &req,
        "progress",
        channel.inner(),
    )
    .await
    .map(|_| ())
    .map_err(|e| e.message)
}

/// Recursively uploads a local directory into the vault under `vault_parent_path`.
///
/// Creates a vault directory node for `dir_path`, then iterates its children:
/// subdirectories are processed recursively; files are uploaded via `upload_one_file`.
/// Returns the first error encountered, if any.
async fn upload_directory(
    dir_path: String,
    dir_name: String,
    vault_parent_path: String,
    set_upload_channel: WriteSignal<Option<IpcChannel<ProgressUpdate>>>,
) -> Result<(), String> {
    // Build the vault path for the new directory node.
    let vault_dir_path = if vault_parent_path.is_empty() {
        dir_name.clone()
    } else {
        format!("{}/{}", vault_parent_path.trim_end_matches('/'), dir_name)
    };

    // Create the directory node in the vault.
    let create_req = CreateVaultDirectoryRequest {
        vault_path: vault_dir_path.clone(),
    };
    let dir_entry = invoke_command::<CreateVaultDirectoryRequest, FileEntry>(
        "create_vault_directory",
        &create_req,
    )
    .await
    .map_err(|e| e.message)?;

    // Use the newly created directory's UUID as the vault parent for children.
    let child_parent = dir_entry.id.clone();

    // List immediate children of the local directory.
    let list_req = ListLocalDirectoryRequest {
        path: dir_path.clone(),
    };
    let children = invoke_command::<ListLocalDirectoryRequest, Vec<LocalEntry>>(
        "list_local_directory",
        &list_req,
    )
    .await
    .map_err(|e| e.message)?;

    for child in children {
        if child.is_dir {
            Box::pin(upload_directory(
                child.path,
                child.name,
                child_parent.clone(),
                set_upload_channel,
            ))
            .await?;
        } else {
            let file_vault_path = format!("{}/{}", child_parent.trim_end_matches('/'), child.name);
            upload_one_file(child.path, file_vault_path, set_upload_channel).await?;
        }
    }
    Ok(())
}

/// File drop zone that accepts dragged files and folders and initiates upload.
///
/// Subscribes to `onDragDropEvent` once in the component body (not inside an
/// `Effect`) and unsubscribes via `on_cleanup`.  Using `Effect::new` caused
/// the listener to be re-registered on each effect re-run; if the async Tauri
/// unlisten promise had not yet resolved, the previous listener was never
/// removed, leaving two active listeners that both triggered `upload_file`.
/// Shows a progress modal for the most-recently-started upload.
#[component]
pub fn DropZone(children: Children) -> impl IntoView {
    let vault = use_vault();
    let vault_actions = use_vault_actions();
    let (upload_channel, set_upload_channel) = signal::<Option<IpcChannel<ProgressUpdate>>>(None);

    // Register the listener once in the component body (not inside Effect::new).
    // Effect::new can re-run if any tracked signal changes, and if the async
    // promise that stores the Tauri unlisten function hasn't resolved before
    // the cleanup fires, the old listener leaks — leaving two active listeners
    // that both invoke upload_file on each drop.
    let unsub = on_file_drop(move |paths| {
        // If the component has been unmounted (user navigated away) the signals
        // are disposed; try_get_untracked() returns None in that case so we can
        // bail out safely instead of panicking.  Also guards against a second
        // listener or Tauri double-fire while an upload is already in progress.
        let Some(in_flight) = upload_channel.try_get_untracked() else {
            return;
        };
        if in_flight.is_some() {
            return;
        }
        let Some(vault_state) = vault.try_get_untracked() else {
            return;
        };
        let current_path = vault_state.current_path;
        let va = vault_actions;
        // Process dropped paths sequentially in a single task so uploads do not
        // race on upload_channel and navigation fires exactly once at the end.
        leptos::task::spawn_local(async move {
            for source_path in paths {
                // Determine whether the dropped path is a directory or a file.
                let stat_req = StatLocalPathRequest {
                    path: source_path.clone(),
                };
                let is_dir = match invoke_command::<StatLocalPathRequest, bool>(
                    "stat_local_path",
                    &stat_req,
                )
                .await
                {
                    Ok(v) => v,
                    Err(err) => {
                        set_upload_channel.set(None);
                        va.set_error(err.message);
                        return;
                    }
                };

                if is_dir {
                    // Extract directory name for the vault node.
                    let dir_name = source_path
                        .split(['/', '\\'])
                        .next_back()
                        .unwrap_or("folder")
                        .to_owned();
                    if let Err(msg) = upload_directory(
                        source_path.clone(),
                        dir_name,
                        current_path.clone(),
                        set_upload_channel,
                    )
                    .await
                    {
                        set_upload_channel.set(None);
                        va.set_error(msg);
                        return;
                    }
                } else {
                    let file_name = source_path.split(['/', '\\']).next_back().unwrap_or("file");
                    let vault_path = join_vault_path(&current_path, file_name)
                        .trim_start_matches('/')
                        .to_owned();
                    if let Err(msg) =
                        upload_one_file(source_path.clone(), vault_path, set_upload_channel).await
                    {
                        set_upload_channel.set(None);
                        va.set_error(msg);
                        return;
                    }
                }
            }
            set_upload_channel.set(None);
            va.navigate(current_path);
        });
    });
    on_cleanup(unsub);

    view! {
        <>
            <div class="relative w-full h-full">
                {children()}
            </div>
            <Show when=move || upload_channel.get().is_some() fallback=|| ()>
                {move || {
                    upload_channel.get().map(|ch| {
                        view! {
                            <ProgressModal
                                channel=ch
                                title="Uploading file"
                                on_close=move || set_upload_channel.set(None)
                            />
                        }
                    })
                }}
            </Show>
        </>
    }
}

// ─── UploadButton ────────────────────────────────────────────────────────────

/// Upload button that opens a native file picker and uploads the selected file.
///
/// Shows a progress modal while the upload is in flight.
#[component]
pub fn UploadButton() -> impl IntoView {
    let vault = use_vault();
    let vault_actions = use_vault_actions();
    let (loading, set_loading) = signal(false);
    let (upload_channel, set_upload_channel) = signal::<Option<IpcChannel<ProgressUpdate>>>(None);

    let on_click = move |_| {
        let vault_actions = vault_actions;
        let set_loading = set_loading;
        leptos::task::spawn_local(async move {
            let Some(source_path) = open_file_dialog().await else {
                return;
            };
            let current_path = vault.get_untracked().current_path;
            let file_name = source_path.split(['/', '\\']).next_back().unwrap_or("file");
            let vault_path = join_vault_path(&current_path, file_name)
                .trim_start_matches('/')
                .to_owned();
            set_loading.set(true);
            let channel = IpcChannel::<ProgressUpdate>::new();
            set_upload_channel.set(Some(channel.clone()));
            let req = UploadFileRequest {
                source_path,
                vault_path,
            };
            match invoke_command_with_channel::<UploadFileRequest, FileEntry>(
                "upload_file",
                &req,
                "progress",
                channel.inner(),
            )
            .await
            {
                Ok(_) => vault_actions.navigate(current_path),
                Err(err) => vault_actions.set_error(err.message),
            }
            set_loading.set(false);
        });
    };

    view! {
        <>
            <Button loading=loading on_click=on_click>"Upload File"</Button>
            <Show when=move || upload_channel.get().is_some() fallback=|| ()>
                {move || {
                    upload_channel.get().map(|ch| {
                        view! {
                            <ProgressModal
                                channel=ch
                                title="Uploading file"
                                on_close=move || set_upload_channel.set(None)
                            />
                        }
                    })
                }}
            </Show>
        </>
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

    // Fetch destinations once to determine whether sharing is supported for the
    // primary destination.  Refreshed automatically when the component mounts.
    let destinations_resource = LocalResource::new(move || async move {
        invoke_command::<(), Vec<DestinationEntry>>("list_destinations", &())
            .await
            .unwrap_or_default()
    });

    let sharing_supported = Signal::derive(move || {
        destinations_resource
            .get()
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|e| e.is_primary)
                    .map(|e| e.sharing_supported)
            })
            .unwrap_or(false)
    });

    view! {
        <div class="flex flex-col gap-4 h-full">
            <div class="flex items-center justify-between">
                <Breadcrumbs path=current_path />
                <UploadButton />
            </div>

            {move || vault.read().error.clone().map(|e| view! {
                <p class="text-danger text-sm">{e}</p>
            })}

            <Show
                when=move || vault.read().loading
                fallback=move || {
                    view! {
                        <DropZone>
                            <FileList
                                entries=Signal::derive(move || vault.read().files.clone())
                                sharing_supported=sharing_supported
                            />
                        </DropZone>
                    }
                }
            >
                <div class="flex justify-center p-8">
                    <Spinner size="h-8 w-8" />
                </div>
            </Show>
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
        assert!(!extension_is_previewable("video.wmv"));
    }

    #[test]
    fn test_extension_is_previewable_case_insensitive() {
        assert!(extension_is_previewable("FILE.TXT"));
        assert!(extension_is_previewable("Photo.PNG"));
        assert!(extension_is_previewable("readme.MD"));
    }
}
