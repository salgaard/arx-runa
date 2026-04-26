//! Toast notification system — displays temporary or persistent feedback messages.
//!
//! Toasts are rendered as a fixed stack in the top-right corner. Each toast has a type
//! (success, error, info, warning) that determines styling and auto-dismiss behavior.
//!
//! # Usage
//!
//! Access toasts via `use_toast()` context hook:
//!
//! ```ignore
//! let toast = use_toast();
//! toast.success("Operation successful!");
//! toast.error("Something went wrong");
//! ```

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Toast notification type, determining visual style and auto-dismiss behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastType {
    /// Success: green/rune, auto-dismisses after 3 seconds.
    Success,
    /// Error: red/danger, persistent (must dismiss manually).
    Error,
    /// Info: blue/rune, auto-dismisses after 2 seconds.
    Info,
    /// Warning: orange, auto-dismisses after 4 seconds.
    Warning,
}

impl ToastType {
    /// Returns the CSS class name for this toast type.
    fn css_class(&self) -> &'static str {
        match self {
            ToastType::Success => "toast-success",
            ToastType::Error => "toast-error",
            ToastType::Info => "toast-info",
            ToastType::Warning => "toast-warning",
        }
    }

    /// Returns the auto-dismiss duration in milliseconds, or `None` if persistent.
    fn auto_dismiss_millis(&self) -> Option<u32> {
        match self {
            ToastType::Success => Some(3000),
            ToastType::Error => None,
            ToastType::Info => Some(2000),
            ToastType::Warning => Some(4000),
        }
    }
}

/// Internal toast item stored in the provider's signal.
#[derive(Clone, Debug)]
pub(crate) struct ToastItem {
    pub id: u64,
    pub message: String,
    pub toast_type: ToastType,
}

/// Single toast notification card.
#[component]
fn ToastNotification(toast: ToastItem, on_dismiss: impl Fn() + 'static) -> impl IntoView {
    let toast_type = toast.toast_type;
    let on_dismiss_rc = std::rc::Rc::new(on_dismiss);
    let dismissed_signal = RwSignal::new(false);

    // Auto-dismiss if duration is configured.
    if let Some(millis) = toast_type.auto_dismiss_millis() {
        let on_dismiss_for_effect = on_dismiss_rc.clone();
        Effect::new(move |_| {
            if !dismissed_signal.get()
                && let Some(window) = web_sys::window()
            {
                dismissed_signal.set(true);
                let on_dismiss_clone = on_dismiss_for_effect.clone();
                let duration_millis = millis as i32;

                let closure = Closure::once(move || {
                    on_dismiss_clone();
                });
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    closure.as_ref().unchecked_ref(),
                    duration_millis,
                );
                closure.forget();
            }
        });
    }

    let css_class = toast_type.css_class();
    let icon = match toast_type {
        ToastType::Success => "✓",
        ToastType::Error => "✕",
        ToastType::Info => "ⓘ",
        ToastType::Warning => "⚠",
    };

    view! {
        <div class=format!("toast-item {}", css_class)>
            <div class="flex items-center gap-3 w-full">
                <span class="text-lg font-bold flex-shrink-0">{icon}</span>
                <span class="text-sm flex-1 break-words">{toast.message.clone()}</span>
                {if matches!(toast_type, ToastType::Error) {
                    view! {
                        <button
                            class="ml-2 text-xs hover:opacity-70 transition-opacity flex-shrink-0"
                            on:click=move |_| {
                                on_dismiss_rc();
                                dismissed_signal.set(true);
                            }
                        >
                            "✕"
                        </button>
                    }
                    .into_any()
                } else {
                    ().into_any()
                }}
            </div>
        </div>
    }
}

/// Renders the stack of active toasts in the top-right corner.
#[component]
pub(crate) fn ToastContainer(
    toasts: ReadSignal<Vec<ToastItem>>,
    on_dismiss: impl Fn(u64) + 'static + Clone + Send,
) -> impl IntoView {
    view! {
        <div class="fixed top-4 right-4 z-[9999] pointer-events-none space-y-2 w-96 max-w-[calc(100vw-2rem)]">
            <For
                each=move || toasts.get()
                key=|t| t.id
                children=move |toast| {
                    let toast_clone = toast.clone();
                    let id = toast.id;
                    let on_dismiss_inner = on_dismiss.clone();
                    view! {
                        <div class="pointer-events-auto">
                            <ToastNotification
                                toast=toast_clone
                                on_dismiss=move || on_dismiss_inner(id)
                            />
                        </div>
                    }
                }
            />
        </div>
    }
}

/// Toast context actions — add, dismiss, and clear all toasts.
///
/// Store these as boxed functions to allow closures with captures.
#[derive(Clone)]
pub struct ToastActions {
    pub(crate) add: std::sync::Arc<dyn Fn(String, ToastType) + Send + Sync>,
    pub(crate) dismiss: std::sync::Arc<dyn Fn(u64) + Send + Sync>,
    #[allow(dead_code)]
    pub(crate) clear_all: std::sync::Arc<dyn Fn() + Send + Sync>,
}

impl ToastActions {
    /// Show a success toast.
    pub fn success(&self, message: impl Into<String>) {
        (self.add)(message.into(), ToastType::Success);
    }

    /// Show an error toast (persistent).
    pub fn error(&self, message: impl Into<String>) {
        (self.add)(message.into(), ToastType::Error);
    }

    /// Show an info toast.
    pub fn info(&self, message: impl Into<String>) {
        (self.add)(message.into(), ToastType::Info);
    }

    /// Show a warning toast.
    pub fn warning(&self, message: impl Into<String>) {
        (self.add)(message.into(), ToastType::Warning);
    }
}

/// Toast context type.
type ToastContextType = (ReadSignal<Vec<ToastItem>>, ToastActions);

/// Global toast context.
pub(crate) static TOAST_CONTEXT: std::sync::OnceLock<ToastContextType> = std::sync::OnceLock::new();

/// Retrieve toast actions from context.
///
/// # Panics
///
/// Panics if called outside a `ToastProvider` tree.
pub fn use_toast() -> ToastActions {
    TOAST_CONTEXT
        .get()
        .expect("Toast context not initialized. Ensure <ToastProvider> wraps the component tree.")
        .1
        .clone()
}

/// Toast provider — wraps the application and manages toast state.
///
/// Must be placed high in the component tree so toasts are accessible everywhere.
///
/// # Example
///
/// ```ignore
/// #[component]
/// fn App() -> impl IntoView {
///     view! {
///         <ToastProvider>
///             <Router ... />
///         </ToastProvider>
///     }
/// }
/// ```
#[component]
pub fn ToastProvider(children: Children) -> impl IntoView {
    let (toasts, set_toasts) = signal(Vec::<ToastItem>::new());

    let add_toast = {
        std::sync::Arc::new(move |message: String, toast_type: ToastType| {
            set_toasts.update(|t| {
                let id = t.len() as u64;
                let item = ToastItem {
                    id,
                    message,
                    toast_type,
                };
                t.push(item);
            });
        })
    };

    let dismiss_toast = {
        std::sync::Arc::new(move |id: u64| {
            set_toasts.update(|t| t.retain(|item| item.id != id));
        })
    };

    let clear_all_toasts = {
        std::sync::Arc::new(move || {
            set_toasts.update(|t| t.clear());
        })
    };

    let actions = ToastActions {
        add: add_toast,
        dismiss: dismiss_toast,
        clear_all: clear_all_toasts,
    };

    let _ = TOAST_CONTEXT.set((toasts, actions.clone()));

    view! {
        <>
            <ToastContainer toasts=toasts on_dismiss=move |id| {
                (actions.dismiss)(id);
            } />
            {children()}
        </>
    }
}

/// Inject toast stylesheet into document head.
///
/// Call this once during app initialization (usually in `App::view()`).
pub fn inject_toast_styles() {
    let style_content = r#"
.toast-item {
  display: flex;
  padding: 12px 16px;
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  font-family: inherit;
  animation: slideInRight 0.3s ease-out;
  max-width: 384px;
}

@keyframes slideInRight {
  from {
    opacity: 0;
    transform: translateX(100%);
  }
  to {
    opacity: 1;
    transform: translateX(0);
  }
}

.toast-success {
  background-color: #00d084;
  color: #fffbf5;
  border-left: 4px solid #009966;
}

.toast-error {
  background-color: #d84855;
  color: #fffbf5;
  border-left: 4px solid #a0364a;
}

.toast-info {
  background-color: #00d084;
  color: #fffbf5;
  border-left: 4px solid #009966;
}

.toast-warning {
  background-color: #ff9800;
  color: #fffbf5;
  border-left: 4px solid #f57c00;
}
"#;

    if let Some(document) = web_sys::window().and_then(|w| w.document())
        && let Ok(Some(head_element)) = document.query_selector("head")
        && let Ok(style_element) = document.create_element("style")
    {
        style_element.set_text_content(Some(style_content));
        let _ = head_element.append_child(&style_element);
    }
}
