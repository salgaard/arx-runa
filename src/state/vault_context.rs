use leptos::prelude::*;

use crate::invoke::invoke_command;
use crate::ipc_types::{FileEntry, ListDirectoryRequest};

/// Current vault-browser state held in Leptos signals.
#[derive(Clone, Debug, Default)]
pub struct VaultState {
    /// Absolute vault-relative path of the directory currently shown.
    pub current_path: String,
    /// Ordered list of entries returned by the last `list_directory` call.
    pub files: Vec<FileEntry>,
    /// `true` while a `navigate` or `list_directory` IPC call is in-flight.
    /// UI-owned: not overwritten by the navigation result on generation mismatch.
    pub loading: bool,
    /// Last user-displayable vault operation error, or `None` when clean.
    pub error: Option<String>,
    /// Set of [`FileEntry::id`] values the user has selected in the UI.
    pub selected: Vec<String>,
}

impl VaultState {
    /// Wipes all fields to their defaults. Called on lock to satisfy Zero-Trace.
    pub fn clear(&mut self) {
        self.current_path.clear();
        self.files.clear();
        self.loading = false;
        self.error = None;
        self.selected.clear();
    }
}

/// Write-side handle to `VaultState` exposing intent-level operations.
///
/// `nav_generation` is a monotonically incrementing `u32` (wrapping) that tags each
/// `navigate()` call. The spawned future discards its result if a newer navigation has
/// superseded it, preventing stale IPC responses from overwriting current state.
#[derive(Clone, Copy)]
pub struct VaultActions {
    set_state: WriteSignal<VaultState>,
    /// Concurrency guard for `navigate()`. Incremented synchronously on each call;
    /// the in-flight future checks equality before committing state.
    nav_generation: RwSignal<u32>,
}

impl VaultActions {
    /// Navigates to `path` by invoking `list_directory` and replacing `files`.
    ///
    /// Concurrent calls are safe: only the response whose generation token still
    /// matches the live counter at completion time is committed. Stale responses
    /// are discarded silently; the current navigation's completion branch is the
    /// sole authority for clearing `loading`.
    pub fn navigate(self, path: String) {
        let set_state = self.set_state;
        let nav_generation = self.nav_generation;

        let nav_gen = nav_generation.get_untracked().wrapping_add(1);
        nav_generation.set(nav_gen);

        leptos::task::spawn_local(async move {
            set_state.update(|s| {
                s.loading = true;
                s.error = None;
            });

            match invoke_command::<ListDirectoryRequest, Vec<FileEntry>>(
                "list_directory",
                &ListDirectoryRequest { path: path.clone() },
            )
            .await
            {
                Ok(files) => {
                    if nav_generation.get_untracked() == nav_gen {
                        set_state.update(|s| {
                            s.current_path = path;
                            s.files = files;
                            s.loading = false;
                        });
                    }
                    // else: stale response — discard silently.
                }
                Err(err) => {
                    if nav_generation.get_untracked() == nav_gen {
                        set_state.update(|s| {
                            s.loading = false;
                            s.error = Some(err.message);
                        });
                    }
                    // else: stale response — discard silently.
                }
            }
        });
    }

    /// Clears all vault state fields to defaults (used on session lock).
    ///
    /// `nav_generation` is incremented first so that any `navigate()` future
    /// already suspended at `invoke_command.await` will detect a generation
    /// mismatch on resumption and discard its payload without writing back
    /// into the cleared state (Zero-Trace, D-001).
    pub fn clear(self) {
        self.nav_generation.update(|g| *g = g.wrapping_add(1));
        self.set_state.update(|s| s.clear());
    }
}

/// Accessor for `VaultState` read side.
pub fn use_vault() -> ReadSignal<VaultState> {
    use_context::<ReadSignal<VaultState>>().expect(
        "VaultProvider must wrap the component tree — did you forget to mount it in src/app.rs?",
    )
}

/// Accessor for `VaultActions`.
pub fn use_vault_actions() -> VaultActions {
    use_context::<VaultActions>().expect(
        "VaultProvider must wrap the component tree — did you forget to mount it in src/app.rs?",
    )
}

/// Provides `ReadSignal<VaultState>` and `VaultActions` to descendants.
#[component]
pub fn VaultProvider(children: Children) -> impl IntoView {
    let (state, set_state) = signal(VaultState::default());
    let nav_generation: RwSignal<u32> = RwSignal::new(0u32);
    provide_context(state);
    provide_context(VaultActions {
        set_state,
        nav_generation,
    });
    children()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file_entry() -> FileEntry {
        FileEntry {
            id: "f1".to_string(),
            name: "readme.md".to_string(),
            entry_type: "file".to_string(),
            size_bytes: 1024,
            modified_at: "2024-01-01T00:00:00Z".to_string(),
            parent_id: None,
        }
    }

    #[test]
    fn test_vault_state_clear_zeroes_all_fields() {
        let mut state = VaultState {
            current_path: "/docs".to_string(),
            files: vec![make_file_entry()],
            loading: true,
            error: Some("disk error".to_string()),
            selected: vec!["f1".to_string()],
        };

        state.clear();

        let default = VaultState::default();
        assert_eq!(state.current_path, default.current_path);
        assert_eq!(state.files.len(), default.files.len());
        assert_eq!(state.loading, default.loading);
        assert_eq!(state.error, default.error);
        assert_eq!(state.selected.len(), default.selected.len());
    }

    #[test]
    fn test_vault_state_clear_resets_selected_and_path_and_error() {
        let mut state = VaultState {
            current_path: "/secret/docs".to_string(),
            selected: vec!["id-a".to_string(), "id-b".to_string()],
            error: Some("network timeout".to_string()),
            ..Default::default()
        };

        state.clear();

        assert!(state.current_path.is_empty());
        assert!(state.selected.is_empty());
        assert_eq!(state.error, None);
    }

    // --- Edge-case / idempotency coverage ---

    /// Calling `clear` on an already-default `VaultState` must leave every field
    /// at its default value — the operation must be safe to call unconditionally.
    #[test]
    fn test_vault_state_clear_on_default_is_idempotent() {
        let mut state = VaultState::default();
        state.clear();
        let expected = VaultState::default();
        assert_eq!(state.current_path, expected.current_path);
        assert_eq!(state.files.len(), expected.files.len());
        assert_eq!(state.loading, expected.loading);
        assert_eq!(state.error, expected.error);
        assert_eq!(state.selected.len(), expected.selected.len());
    }
}
