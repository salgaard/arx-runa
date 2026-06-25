//! VaultPicker — home screen listing locally-known vaults.
//!
//! Shown before authentication. Calls `list_vaults` IPC (no auth required)
//! and lets the user pick a vault to unlock or start creating a new one.

use leptos::prelude::*;

use crate::invoke::invoke_command;
use crate::ipc_types::VaultSummary;

/// Home screen component that lists locally-known vaults.
#[component]
pub fn VaultPicker(
    /// Called with the chosen vault when the user clicks a vault card.
    on_select: impl Fn(VaultSummary) + Send + Sync + 'static + Clone,
    /// Called when the user clicks "Create vault".
    on_create: impl Fn() + Send + Sync + 'static + Clone,
    /// Called when the user clicks "Recover vault from cloud".
    on_recover: impl Fn() + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let vaults: RwSignal<Vec<VaultSummary>> = RwSignal::new(Vec::new());
    let loading = RwSignal::new(true);

    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            match invoke_command::<(), Vec<VaultSummary>>("list_vaults", &()).await {
                Ok(list) => {
                    vaults.set(list);
                }
                Err(err) => {
                    leptos::logging::warn!("list_vaults failed: {}", err.message);
                }
            }
            loading.set(false);
        });
    });

    view! {
        <div class="min-h-full bg-iron flex items-center justify-center p-4">
            <div class="w-full max-w-md">
                <h1 class="text-3xl text-bone text-center mb-2">"Arx Runa"</h1>
                <p class="text-text-secondary text-center text-sm mb-8">
                    "Zero-knowledge file encryption"
                </p>

                {move || {
                    if loading.get() {
                        return view! {
                            <div class="text-center text-text-secondary py-12">
                                "Loading vaults\u{2026}"
                            </div>
                        }.into_any();
                    }

                    let vault_list = vaults.get();
                    if vault_list.is_empty() {
                        return view! {
                            <div class="bg-stone border border-steel rounded-xl p-8 text-center mb-4">
                                <p class="text-text-secondary mb-4">"No vaults found on this device."</p>
                                <p class="text-text-secondary text-sm">
                                    "Create a vault to get started."
                                </p>
                            </div>
                        }.into_any();
                    }

                    let cards: Vec<_> = vault_list
                        .into_iter()
                        .map(|vault| {
                            let display_name = vault.name.clone()
                                .unwrap_or_else(|| format!("{}…", &vault.vault_id[..8]));
                            let tier_label = if vault.tier == 2 {
                                "Password + Key file"
                            } else {
                                "Password only"
                            };
                            let on_select = on_select.clone();
                            let vault_clone = vault.clone();
                            view! {
                                <button
                                    type="button"
                                    class="w-full bg-stone border border-steel rounded-xl p-4 text-left cursor-pointer hover:border-rune hover:bg-surface-overlay transition-colors"
                                    data-testid="vault-card"
                                    on:click=move |_| on_select(vault_clone.clone())
                                >
                                    <div class="flex items-center justify-between">
                                        <span class="text-bone font-medium">{display_name}</span>
                                        <span class="text-xs text-text-secondary bg-surface-overlay border border-border-default rounded px-2 py-0.5">
                                            {tier_label}
                                        </span>
                                    </div>
                                </button>
                            }
                        })
                        .collect();

                    view! {
                        <div class="space-y-3 mb-4">{cards}</div>
                    }.into_any()
                }}

                <button
                    type="button"
                    class="w-full px-4 py-3 rounded-xl border border-rune text-rune cursor-pointer hover:bg-surface-overlay transition-colors text-sm font-medium"
                    data-testid="create-vault-button"
                    on:click={
                        let on_create = on_create.clone();
                        move |_| on_create()
                    }
                >
                    "+ Create new vault"
                </button>

                <button
                    type="button"
                    class="w-full px-4 py-2 rounded-xl text-text-secondary cursor-pointer hover:text-bone transition-colors text-sm mt-2"
                    on:click={
                        let on_recover = on_recover.clone();
                        move |_| on_recover()
                    }
                >
                    "\u{2193} Recover vault from cloud"
                </button>
            </div>
        </div>
    }
}
