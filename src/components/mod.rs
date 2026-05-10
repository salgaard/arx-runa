//! Shared UI primitives — leaf module, no project-internal dependencies.
//!
//! All components in this module are stateless primitives; business logic lives
//! in the `auth`, `vault`, `transfer`, and `layout` modules.

mod button;
mod chunk_size_selector;
mod destination_selector;
mod epoch_buffer_toggle;
mod input;
mod modal;
mod spinner;
mod storage_selector;
mod sync_conflict_dialog;
mod toast;

pub use button::Button;
pub use chunk_size_selector::{CHUNK_MAX, CHUNK_MIN, ChunkSizeSelector, PRESETS, clamp_chunk_size};
pub use destination_selector::{DestinationKind, DestinationSelector};
pub use epoch_buffer_toggle::EpochBufferToggle;
pub use input::Input;
pub use modal::Modal;
pub use spinner::Spinner;
pub use storage_selector::{StorageConfig, StorageProvider, StorageSelector};
pub use sync_conflict_dialog::SyncConflictDialog;
pub use toast::{ToastActions, ToastProvider, ToastType, inject_toast_styles, use_toast};
