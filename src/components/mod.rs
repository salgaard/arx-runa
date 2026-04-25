//! Shared UI primitives — leaf module, no project-internal dependencies.
//!
//! All components in this module are stateless primitives; business logic lives
//! in the `auth`, `vault`, `transfer`, and `layout` modules.

mod button;
mod input;
mod modal;
mod spinner;
mod toast;
mod storage_selector;

pub use button::Button;
pub use input::Input;
pub use modal::Modal;
pub use spinner::Spinner;
pub use toast::{ToastActions, ToastProvider, ToastType, inject_toast_styles, use_toast};
pub use storage_selector::{StorageSelector, StorageProvider, StorageConfig};
