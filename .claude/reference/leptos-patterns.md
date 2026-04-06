# Leptos Patterns Reference

This document provides extended code examples for Leptos patterns in Arx Runa.
For rules and requirements, see `.github/instructions/leptos.instructions.md`.

## Table of Contents

1. [Signal Access Methods](#signal-access-methods)
2. [Component Props Patterns](#component-props-patterns)
3. [List Rendering](#list-rendering)
4. [Form Patterns](#form-patterns)
5. [Async Patterns](#async-patterns)
6. [Global State with Stores](#global-state-with-stores)
7. [Tauri IPC Patterns](#tauri-ipc-patterns)
8. [Styling with Tailwind](#styling-with-tailwind)

---

## Signal Access Methods

Leptos provides multiple ways to access signal values. Choose based on your needs:

```rust
let (items, set_items) = signal(vec![1, 2, 3]);

// .get() — clones the value, tracks dependency
let cloned: Vec<i32> = items.get();

// .read() — borrows the value (returns guard), tracks dependency
// Use when you don't need ownership
let len = items.read().len();

// .with(|val| ...) — access via callback without cloning
// Returns callback's return value
let first = items.with(|v| v.first().copied());

// .set() — replaces the entire value
set_items.set(vec![4, 5, 6]);

// .write() — returns mutable guard for in-place mutation
// Notifies subscribers when guard drops
set_items.write().push(7);

// .update(|val| ...) — mutate via callback
set_items.update(|v| v.retain(|x| *x > 3));
```

### When to use which

| Method | Clones? | Use when |
|--------|---------|----------|
| `.get()` | Yes | Need owned value, value is cheap to clone |
| `.read()` | No | Need to inspect, value is expensive to clone |
| `.with()` | No | Need computed value from contents |
| `.set()` | — | Replacing entire value |
| `.write()` | — | Mutating in place |
| `.update()` | — | Mutating via closure |

---

## Component Props Patterns

### Basic props with defaults

```rust
/// A button with loading state indicator.
#[component]
fn LoadingButton(
    /// Button label text.
    label: String,
    
    /// Whether the button shows a loading spinner.
    #[prop(default = false)]
    loading: bool,
    
    /// Click handler.
    on_click: impl Fn() + 'static,
) -> impl IntoView {
    view! {
        <button
            disabled=loading
            on:click=move |_| on_click()
        >
            {if loading { "Loading..." } else { &label }}
        </button>
    }
}
```

### Reactive props with Signal wrapper

```rust
/// A text display that updates reactively.
#[component]
fn ReactiveDisplay(
    /// Any signal-like source that produces a String.
    #[prop(into)]
    text: Signal<String>,
) -> impl IntoView {
    view! { <span>{text}</span> }
}

// Can be called with:
// - ReadSignal<String>
// - RwSignal<String>
// - Memo<String>
// - closure that returns String
```

### Generic props for maximum flexibility

```rust
/// A wrapper that calls a render function.
#[component]
fn RenderProp<F, V>(render: F) -> impl IntoView
where
    F: Fn() -> V + 'static,
    V: IntoView,
{
    view! { {render()} }
}
```

---

## List Rendering

### Static list (known at compile time)

```rust
let items = vec!["Apple", "Banana", "Cherry"];

view! {
    <ul>
        {items.into_iter()
            .map(|item| view! { <li>{item}</li> })
            .collect::<Vec<_>>()}
    </ul>
}
```

### Dynamic list with signal

```rust
let (items, set_items) = signal(vec![
    ("a", "Apple"),
    ("b", "Banana"),
]);

view! {
    <ul>
        // Re-renders entire list when items changes
        {move || items.get().into_iter()
            .map(|(key, name)| view! { <li>{name}</li> })
            .collect::<Vec<_>>()}
    </ul>
}
```

### Keyed list with <For/> (efficient updates)

```rust
#[derive(Clone, PartialEq)]
struct Todo {
    id: u32,
    text: String,
    done: bool,
}

let (todos, set_todos) = signal(vec![
    Todo { id: 1, text: "Learn Leptos".into(), done: false },
    Todo { id: 2, text: "Build Arx Runa".into(), done: false },
]);

view! {
    <ul>
        <For
            // Data source
            each=move || todos.get()
            // Unique key for each item (for efficient diffing)
            key=|todo| todo.id
            // Render function receives owned item
            children=|todo| view! {
                <li class:done=todo.done>
                    {todo.text}
                </li>
            }
        />
    </ul>
}
```

---

## Form Patterns

### Controlled form with validation

```rust
#[component]
fn LoginForm(on_submit: impl Fn(String, String) + 'static) -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    
    // Derived validation
    let is_valid = move || {
        !username.read().is_empty() && password.read().len() >= 8
    };
    
    let handle_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        
        if !is_valid() {
            set_error.set(Some("Invalid credentials".into()));
            return;
        }
        
        on_submit(username.get(), password.get());
    };
    
    view! {
        <form on:submit=handle_submit>
            <div>
                <label>"Username"</label>
                <input
                    type="text"
                    prop:value=username
                    on:input:target=move |ev| set_username.set(ev.target().value())
                />
            </div>
            <div>
                <label>"Password"</label>
                <input
                    type="password"
                    prop:value=password
                    on:input:target=move |ev| set_password.set(ev.target().value())
                />
            </div>
            
            // Error display
            {move || error.get().map(|e| view! {
                <p class="error">{e}</p>
            })}
            
            <button type="submit" disabled=move || !is_valid()>
                "Log In"
            </button>
        </form>
    }
}
```

### Uncontrolled form with NodeRef

```rust
#[component]
fn SearchForm() -> impl IntoView {
    let input_ref = NodeRef::<html::Input>::new();
    
    let handle_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        
        if let Some(input) = input_ref.get() {
            let query = input.value();
            // Do something with query
            logging::log!("Searching for: {}", query);
        }
    };
    
    view! {
        <form on:submit=handle_submit>
            <input type="text" node_ref=input_ref placeholder="Search..." />
            <button type="submit">"Search"</button>
        </form>
    }
}
```

---

## Async Patterns

### Resource for data loading

```rust
/// Vault contents that refresh when vault_id changes.
#[component]
fn VaultContents(vault_id: ReadSignal<String>) -> impl IntoView {
    let contents = LocalResource::new(move || {
        let id = vault_id.get();
        async move {
            invoke::<_, Vec<FileEntry>>("list_contents", &ListRequest { vault_id: id }).await
        }
    });
    
    view! {
        {move || match contents.get() {
            None => view! { <p>"Loading..."</p> }.into_any(),
            Some(Ok(files)) => view! {
                <FileList files=files />
            }.into_any(),
            Some(Err(e)) => view! {
                <p class="error">"Error: " {e.to_string()}</p>
            }.into_any(),
        }}
    }
}
```

### Action for mutations with optimistic UI

```rust
#[component]
fn DeleteButton(file_id: String) -> impl IntoView {
    let delete_action = Action::new(move |id: &String| {
        let id = id.clone();
        async move {
            invoke::<_, ()>("delete_file", &DeleteRequest { file_id: id }).await
        }
    });
    
    let pending = delete_action.pending();
    
    view! {
        <button
            on:click=move |_| delete_action.dispatch(file_id.clone())
            disabled=pending
        >
            {move || if pending.get() { "Deleting..." } else { "Delete" }}
        </button>
    }
}
```

### Manual async with spawn_local

```rust
#[component]
fn OneTimeLoad() -> impl IntoView {
    let (data, set_data) = signal(Option::<Data>::None);
    let (loading, set_loading) = signal(true);
    
    // Runs once on mount
    spawn_local(async move {
        match fetch_initial_data().await {
            Ok(d) => set_data.set(Some(d)),
            Err(e) => logging::error!("Failed to load: {}", e),
        }
        set_loading.set(false);
    });
    
    view! {
        {move || if loading.get() {
            view! { <p>"Loading..."</p> }.into_any()
        } else {
            data.get().map(|d| view! { <Display data=d /> }.into_any())
                .unwrap_or_else(|| view! { <p>"No data"</p> }.into_any())
        }}
    }
}
```

---

## Global State with Stores

### Define store structure

```rust
use leptos::prelude::*;
use leptos_struct_derive::Store;

#[derive(Clone, Debug, Default, Store)]
pub struct VaultState {
    pub unlocked: bool,
    pub current_path: Vec<String>,
    pub files: Vec<FileEntry>,
    pub selected: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub id: String,
    pub name: String,
    pub is_directory: bool,
}
```

### Provide and consume

```rust
// At app root
#[component]
fn App() -> impl IntoView {
    provide_context(Store::new(VaultState::default()));
    
    view! {
        <VaultBrowser />
    }
}

// In child component
#[component]
fn VaultBrowser() -> impl IntoView {
    let store = expect_context::<Store<VaultState>>();
    
    // Access specific fields (only reacts to that field)
    let files = store.files();
    let unlocked = store.unlocked();
    
    view! {
        <Show when=move || unlocked.get()>
            <FileList files=files />
        </Show>
    }
}

// Update store
fn navigate_to(store: &Store<VaultState>, path: Vec<String>) {
    store.current_path().set(path);
    // Trigger reload...
}
```

---

## Tauri IPC Patterns

### Basic invoke

```rust
use serde::{Deserialize, Serialize};
use tauri_wasm::api::core::invoke;

#[derive(Serialize)]
struct UnlockRequest {
    password: String,
    key_file_path: String,
}

#[derive(Deserialize)]
struct UnlockResponse {
    vault_id: String,
}

async fn unlock_vault(password: String, key_file_path: String) -> Result<String, String> {
    let response: UnlockResponse = invoke("unlock_vault", &UnlockRequest {
        password,
        key_file_path,
    }).await.map_err(|e| e.to_string())?;
    
    Ok(response.vault_id)
}
```

### IPC-backed resource

```rust
#[component]
fn SessionStatus() -> impl IntoView {
    // Poll session status every 30 seconds
    let status = LocalResource::new(|| async {
        invoke::<_, SessionInfo>("get_session_status", &()).await
    });
    
    // Set up periodic refresh
    let (trigger, set_trigger) = signal(0);
    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(30_000).await;
            set_trigger.update(|n| *n += 1);
        }
    });
    
    // Refetch when trigger changes
    let status = LocalResource::new(move || {
        let _ = trigger.get(); // Track trigger
        async { invoke::<_, SessionInfo>("get_session_status", &()).await }
    });
    
    view! {
        {move || status.get().map(|s| match s {
            Ok(info) => view! {
                <span class="status-indicator">
                    {if info.locked { "🔒" } else { "🔓" }}
                </span>
            }.into_any(),
            Err(_) => view! { <span class="status-error">"?"</span> }.into_any(),
        })}
    }
}
```

### Action for IPC mutations

```rust
#[component]
fn UploadButton() -> impl IntoView {
    let upload_action = Action::new(|path: &String| {
        let path = path.clone();
        async move {
            invoke::<_, UploadResult>("upload_file", &UploadRequest { path }).await
        }
    });
    
    let pending = upload_action.pending();
    let result = upload_action.value();
    
    view! {
        <button
            on:click=move |_| {
                spawn_local(async move {
                    // Open file picker via Tauri
                    if let Some(path) = pick_file().await {
                        upload_action.dispatch(path);
                    }
                });
            }
            disabled=pending
        >
            {move || if pending.get() { "Uploading..." } else { "Upload File" }}
        </button>
        
        {move || result.get().map(|r| match r {
            Ok(res) => view! { <p>"Uploaded: " {res.file_name}</p> }.into_any(),
            Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
        })}
    }
}
```

---

## Zero-Trace Practices

### Clear sensitive state on lock

```rust
#[component]
fn VaultApp() -> impl IntoView {
    let store = expect_context::<Store<VaultState>>();
    
    let lock_vault = move || {
        spawn_local(async move {
            // Tell backend to lock
            let _ = invoke::<_, ()>("lock_session", &()).await;
        });
        
        // Clear all sensitive UI state immediately
        store.files().set(vec![]);
        store.current_path().set(vec![]);
        store.selected().set(None);
        store.unlocked().set(false);
    };
    
    // ...
}
```

### Avoid persistent storage

```rust
// ❌ NEVER store sensitive data in browser storage
// window().local_storage().set_item("session_key", key);

// ✅ Keep in signals (memory only)
let (session_key, set_session_key) = signal(Option::<String>::None);

// ✅ Clear on unmount if needed
on_cleanup(move || {
    set_session_key.set(None);
});
```

---

## Styling with Tailwind

Arx Runa uses Tailwind CSS with a custom dark theme. Full design system is in
`docs/architecture/design-system.md`.

### Using CSS component classes

The project defines reusable component classes in `src/styles.css`:

```rust
// Use predefined component classes
view! {
    <button class="btn-primary">"Unlock"</button>
    <button class="btn-secondary">"Cancel"</button>
    <button class="btn-danger">"Delete"</button>
    
    <input type="text" class="input" placeholder="Enter text" />
    <input type="text" class="input-error" />  // Error state
    
    <div class="card">
        // Card content
    </div>
    
    <div class="status-locked">
        <LockIcon class="w-4 h-4" />
        "Locked"
    </div>
    
    <div class="file-item">
        <FileIcon class="w-5 h-5 text-void-400" />
        <span class="font-path">"document.pdf"</span>
    </div>
}
```

### Complete login form example

```rust
#[component]
fn LoginForm() -> impl IntoView {
    let (password, set_password) = signal(String::new());
    let (key_path, set_key_path) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    
    let handle_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);
        
        spawn_local(async move {
            match unlock_vault(password.get(), key_path.get()).await {
                Ok(_) => { /* navigate to vault */ }
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
    };
    
    view! {
        <div class="min-h-screen bg-void-950 flex items-center justify-center p-4">
            <div class="w-full max-w-md">
                <div class="card-elevated">
                    <h1 class="text-2xl font-semibold text-void-50 text-center mb-6">
                        "Unlock Vault"
                    </h1>
                    
                    <form on:submit=handle_submit class="space-y-4">
                        // Key file field
                        <div class="space-y-1">
                            <label class="block text-sm font-medium text-void-200">
                                "USB Key File"
                            </label>
                            <input
                                type="text"
                                class="input font-mono"
                                placeholder="/media/usb/arx-runa.key"
                                prop:value=key_path
                                on:input:target=move |ev| set_key_path.set(ev.target().value())
                            />
                        </div>
                        
                        // Password field
                        <div class="space-y-1">
                            <label class="block text-sm font-medium text-void-200">
                                "Password"
                            </label>
                            <input
                                type="password"
                                class="input"
                                placeholder="Enter your password"
                                prop:value=password
                                on:input:target=move |ev| set_password.set(ev.target().value())
                            />
                        </div>
                        
                        // Error display
                        {move || error.get().map(|e| view! {
                            <div class="p-3 bg-danger/10 border border-danger/30 rounded-lg">
                                <p class="text-danger text-sm">{e}</p>
                            </div>
                        })}
                        
                        // Submit button
                        <button
                            type="submit"
                            class="btn-primary w-full"
                            disabled=loading
                        >
                            {move || if loading.get() {
                                "Unlocking..."
                            } else {
                                "Unlock Vault"
                            }}
                        </button>
                    </form>
                </div>
            </div>
        </div>
    }
}
```

### File browser example

```rust
#[component]
fn FileBrowser() -> impl IntoView {
    let store = expect_context::<Store<VaultState>>();
    let files = store.files();
    let selected = store.selected();
    
    view! {
        <div class="card h-full">
            <div class="flex items-center justify-between mb-4">
                <h2 class="text-lg font-semibold text-void-50">"Files"</h2>
                <div class="status-unlocked">
                    <UnlockIcon class="w-4 h-4" />
                    "Unlocked"
                </div>
            </div>
            
            <div class="space-y-1">
                <For
                    each=move || files.get()
                    key=|f| f.id.clone()
                    children=move |file| {
                        let file_id = file.id.clone();
                        let is_selected = move || selected.get() == Some(file_id.clone());
                        
                        view! {
                            <div
                                class=move || if is_selected() {
                                    "file-item-selected"
                                } else {
                                    "file-item"
                                }
                                on:click=move |_| selected.set(Some(file_id.clone()))
                            >
                                {if file.is_directory {
                                    view! { <FolderIcon class="w-5 h-5 text-accent-400" /> }.into_any()
                                } else {
                                    view! { <FileIcon class="w-5 h-5 text-void-400" /> }.into_any()
                                }}
                                <span class="font-path text-void-100 truncate flex-1">
                                    {file.name.clone()}
                                </span>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}
```

### Color usage quick reference

| Purpose | Tailwind Class |
|---------|----------------|
| App background | `bg-void-950` |
| Card/panel | `bg-void-900` |
| Modal/elevated | `bg-void-800` |
| Hover state | `hover:bg-void-700` |
| Primary text | `text-void-50` |
| Secondary text | `text-void-200` |
| Muted text | `text-void-400` |
| Border | `border-void-700` |
| Primary button | `bg-accent-600` |
| Success/unlocked | `text-secure` |
| Locked state | `text-locked` |
| Warning | `text-warning` |
| Error | `text-danger` |
