//! Transfer progress modal — displays streaming progress from an `IpcChannel`.

use leptos::prelude::*;

use crate::components::Modal;
use crate::ipc_channel::IpcChannel;
use crate::ipc_types::ProgressUpdate;

/// Modal dialog that displays streaming transfer progress from an `IpcChannel`.
///
/// The modal becomes visible when the first `ProgressUpdate` arrives and closes
/// automatically on 100% completion or on IPC error (caller closes via
/// `on_close`).
#[component]
pub fn ProgressModal(
    /// The typed channel to receive `ProgressUpdate` payloads from.
    channel: IpcChannel<ProgressUpdate>,
    /// Human-readable title for the transfer operation.
    #[prop(into)]
    title: String,
    /// Called when the modal should be closed (on completion or user cancel).
    on_close: impl Fn() + 'static + Clone + Send + Sync,
) -> impl IntoView {
    let (progress, set_progress) = signal::<Option<ProgressUpdate>>(None);
    let title_stored = StoredValue::new(title);

    channel.on_message({
        let on_close = on_close.clone();
        move |update| {
            if update.percent >= 100 {
                on_close();
            }
            set_progress.set(Some(update));
        }
    });

    view! {
        <Modal
            open=Signal::derive(move || progress.read().is_some())
            on_close=on_close
        >
            <div data-testid="progress-modal">
                <h2 class="text-lg text-bone mb-4">{move || title_stored.get_value()}</h2>
                {move || progress.read().as_ref().map(|p| {
                    let percent = p.percent;
                    let status = p.status.clone();
                    view! {
                        <div class="mb-2">
                            <div class="h-2 bg-surface-overlay rounded-full overflow-hidden">
                                <div
                                    class="h-full bg-rune transition-all"
                                    style=move || format!("width: {percent}%")
                                />
                            </div>
                        </div>
                        <p class="text-sm text-text-secondary">{status}</p>
                    }
                })}
            </div>
        </Modal>
    }
}
