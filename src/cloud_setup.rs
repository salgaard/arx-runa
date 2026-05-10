//! Cloud storage setup wizard.
//!
//! Shown at app startup when no primary cloud endpoint has been saved to
//! `cloud-config.json`. Users who prefer local-only storage can skip it.

use leptos::prelude::*;

use crate::components::Button;
use crate::invoke::invoke_command;
use crate::ipc_types::ConfigureCloudRequest;

/// Cloud setup modal — guides the user through saving a primary cloud endpoint.
///
/// Renders as a full-screen overlay. Calls `on_configured` on success and
/// `on_skip` when the user chooses local-only storage.
#[component]
pub fn CloudSetupModal(
    /// Called after the cloud endpoint is successfully saved.
    on_configured: impl Fn() + 'static + Clone,
    /// Called when the user dismisses the modal to proceed with local storage.
    on_skip: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let (provider, set_provider) = signal("s3".to_string());
    let (bucket, set_bucket) = signal(String::new());
    let (region, set_region) = signal(String::new());
    let (endpoint, set_endpoint) = signal(String::new());
    let (path_prefix, set_path_prefix) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);

    let field_class = "w-full bg-surface-overlay border border-border-default rounded-lg px-3 py-2 text-bone text-sm focus:outline-none focus:border-rune";

    let on_save = move |_| {
        let provider_val = provider.get();
        let bucket_val = bucket.get();
        let region_val = region.get();
        let endpoint_val = endpoint.get();
        let path_prefix_val = path_prefix.get();
        let on_configured = on_configured.clone();
        let set_loading = set_loading;
        let set_error = set_error;

        if provider_val.is_empty() {
            set_error.set(Some("Provider is required".to_string()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        leptos::task::spawn_local(async move {
            let result = invoke_command::<ConfigureCloudRequest, ()>(
                "configure_cloud",
                &ConfigureCloudRequest {
                    provider: provider_val,
                    bucket: bucket_val,
                    region: region_val,
                    endpoint: endpoint_val,
                    path_prefix: path_prefix_val,
                },
            )
            .await;
            set_loading.set(false);
            match result {
                Ok(()) => on_configured(),
                Err(err) => set_error.set(Some(err.message)),
            }
        });
    };

    let is_google_drive = move || provider.get() == "google_drive";
    let is_s3_or_custom = move || matches!(provider.get().as_str(), "s3" | "rclone");

    view! {
        <div class="fixed inset-0 bg-iron/90 backdrop-blur-sm flex items-center justify-center z-50 p-4">
            <div class="w-full max-w-md bg-stone border border-steel rounded-xl p-6 shadow-2xl">
                <h2 class="text-xl text-bone font-semibold mb-1">"Set up cloud storage"</h2>
                <p class="text-sm text-text-secondary mb-6">
                    "Configure your primary cloud provider so vaults can sync automatically. \
                     You can skip this and use local storage only."
                </p>

                <div class="space-y-4">
                    <div class="space-y-1">
                        <label class="text-xs text-text-secondary">"Provider"</label>
                        <select
                            class=field_class
                            on:change=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_provider.set(event_target_value(&ev));
                                set_bucket.set(String::new());
                                set_region.set(String::new());
                                set_endpoint.set(String::new());
                            }
                        >
                            <option value="s3">"Amazon S3"</option>
                            <option value="b2">"Backblaze B2"</option>
                            <option value="google_drive">"Google Drive"</option>
                            <option value="rclone">"Other (Rclone)"</option>
                        </select>
                    </div>

                    <Show when=move || !is_google_drive()>
                        <div class="space-y-1">
                            <label class="text-xs text-text-secondary">
                                "Bucket"
                            </label>
                            <input
                                type="text"
                                placeholder="my-arx-runa-bucket"
                                value=move || bucket.get()
                                on:input=move |ev| {
                                    use leptos::prelude::event_target_value;
                                    set_bucket.set(event_target_value(&ev));
                                }
                                class=field_class
                            />
                        </div>

                        <Show when=is_s3_or_custom>
                            <div class="space-y-1">
                                <label class="text-xs text-text-secondary">"Region"</label>
                                <input
                                    type="text"
                                    placeholder="us-east-1"
                                    value=move || region.get()
                                    on:input=move |ev| {
                                        use leptos::prelude::event_target_value;
                                        set_region.set(event_target_value(&ev));
                                    }
                                    class=field_class
                                />
                            </div>
                            <div class="space-y-1">
                                <label class="text-xs text-text-secondary">"Endpoint (optional)"</label>
                                <input
                                    type="text"
                                    placeholder="https://s3.example.com"
                                    value=move || endpoint.get()
                                    on:input=move |ev| {
                                        use leptos::prelude::event_target_value;
                                        set_endpoint.set(event_target_value(&ev));
                                    }
                                    class=field_class
                                />
                            </div>
                        </Show>
                    </Show>

                    <Show when=is_google_drive>
                        <div class="p-3 bg-surface-overlay border border-border-default rounded-lg">
                            <p class="text-xs text-text-secondary">
                                "Google Drive uses OAuth authorization. Save this configuration \
                                 now and complete OAuth setup in Settings → Destinations after \
                                 your vault is created."
                            </p>
                        </div>
                    </Show>

                    <div class="space-y-1">
                        <label class="text-xs text-text-secondary">"Path prefix (optional)"</label>
                        <input
                            type="text"
                            placeholder="arx-runa"
                            value=move || path_prefix.get()
                            on:input=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_path_prefix.set(event_target_value(&ev));
                            }
                            class=field_class
                        />
                    </div>

                    {move || error.get().map(|e| view! {
                        <p class="text-danger text-sm">{e}</p>
                    })}
                </div>

                <div class="flex gap-3 mt-6">
                    <button
                        type="button"
                        on:click=move |_| on_skip()
                        class="flex-1 px-4 py-2 rounded-lg border border-border-default text-text-secondary hover:text-bone hover:bg-surface-overlay transition-colors text-sm"
                    >
                        "Skip — local storage only"
                    </button>
                    <Button loading=loading on_click=on_save>
                        "Save Configuration"
                    </Button>
                </div>
            </div>
        </div>
    }
}
