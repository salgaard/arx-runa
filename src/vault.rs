//! Vault file-browser components: breadcrumbs, file list, drag-drop zone, and
//! the upload button.
//!
//! All components read vault state via `use_vault()` and trigger navigation via
//! `use_vault_actions()` from the `VaultProvider` context.

use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::components::{Button, Spinner};
use crate::dialog::open_file_dialog;
use crate::drag_drop::on_file_drop;
use crate::invoke::invoke_command;
use crate::ipc_types::{FileEntry, UploadFileRequest};
use crate::state::{use_vault, use_vault_actions};

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

/// Single row in the file list.
///
/// Directories are rendered with a folder icon and navigate on click.
/// Files are rendered with a file icon and are selected on click.
#[component]
pub fn FileItem(
    /// The file or directory entry to display.
    entry: FileEntry,
) -> impl IntoView {
    let vault = use_vault();
    let actions = use_vault_actions();
    let is_dir = entry.entry_type == "directory";
    let name = entry.name.clone();
    let path = entry.name.clone();

    let on_click = move |_| {
        if is_dir {
            let current = vault.get_untracked().current_path;
            actions.navigate(join_vault_path(&current, &path));
        }
    };

    view! {
        <div
            class="flex items-center gap-3 p-2 rounded hover:bg-surface-overlay cursor-pointer"
            on:click=on_click
        >
            <span class="text-rune w-4 text-center">
                {if is_dir { "📁" } else { "📄" }}
            </span>
            <span class="flex-1 text-bone text-sm">{name}</span>
            <span class="text-text-muted text-xs">
                {if is_dir {
                    String::new()
                } else {
                    format!("{} B", entry.size_bytes)
                }}
            </span>
        </div>
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
/// Uses `UploadFileRequest` per file; progress channel is wired in Phase 7.
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
}
