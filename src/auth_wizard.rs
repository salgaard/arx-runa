//! Multi-step vault creation wizard — guides users through identity, storage, and review steps.
//!
//! This module breaks down the complex VaultCreationPage into manageable steps.

use leptos::prelude::*;
use crate::components::{Button, Input, StorageProvider};

/// Current wizard step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    Identity,
    Storage,
    Review,
}

impl WizardStep {
    pub fn number(&self) -> u32 {
        match self {
            WizardStep::Identity => 1,
            WizardStep::Storage => 2,
            WizardStep::Review => 3,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            WizardStep::Identity => "Vault Identity",
            WizardStep::Storage => "Storage Destination",
            WizardStep::Review => "Review & Create",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            WizardStep::Identity => "Set your vault name, password, and authentication tier",
            WizardStep::Storage => "Choose where your vault data will be stored",
            WizardStep::Review => "Review your choices and create the vault",
        }
    }
}

/// Progress indicator showing current step.
#[component]
pub fn ProgressIndicator(
    current_step: WizardStep,
) -> impl IntoView {
    view! {
        <div class="flex items-center justify-between mb-8">
            <div class="flex items-center gap-4 flex-1">
                {(1..=3).map(|step_num| {
                    let is_current = step_num == current_step.number() as usize;
                    let is_complete = step_num < current_step.number() as usize;
                    
                    let step_enum = match step_num {
                        1 => WizardStep::Identity,
                        2 => WizardStep::Storage,
                        3 => WizardStep::Review,
                        _ => WizardStep::Identity,
                    };

                    view! {
                        <div class="flex items-center">
                            <div class="flex flex-col items-center">
                                <div
                                    class="w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium transition-colors"
                                    class=("bg-rune text-iron", is_current || is_complete)
                                    class=("bg-steel text-text-secondary", !(is_current || is_complete))
                                >
                                    <span>
                                        {if is_complete { "✓".to_string() } else { step_num.to_string() }}
                                    </span>
                                </div>
                                <div class="text-xs text-text-secondary mt-1 text-center max-w-[60px]">
                                    {step_enum.title()}
                                </div>
                            </div>
                            {if step_num < 3 {
                                view! {
                                    <div
                                        class="flex-1 h-1 mx-2 transition-colors"
                                        class=("bg-rune", is_complete)
                                        class=("bg-steel", !is_complete)
                                    />
                                }.into_any()
                            } else {
                                view! { <></> }.into_any()
                            }}
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

/// Identity step — vault name, password, tier, and key file.
#[component]
pub fn IdentityStep(
    vault_name: ReadSignal<String>,
    set_vault_name: WriteSignal<String>,
    password: ReadSignal<String>,
    set_password: WriteSignal<String>,
    tier: ReadSignal<u8>,
    set_tier: WriteSignal<u8>,
    key_file_destination: ReadSignal<Option<String>>,
    on_browse_key: impl Fn(leptos::ev::MouseEvent) + 'static + Clone + Send + Sync,
) -> impl IntoView {
    use leptos::prelude::event_target_value;
    
    view! {
        <div class="space-y-4">
            <Input
                label="Vault Name".to_string()
                value=vault_name
                on_input=move |v| set_vault_name.set(v)
            />
            <Input
                input_type="password"
                label="Password".to_string()
                value=password
                on_input=move |v| set_password.set(v)
            />

            <div class="mb-4">
                <label class="text-sm text-text-secondary block mb-1">"Authentication Tier"</label>
                <select
                    class="bg-surface-overlay border border-border-default rounded-lg px-3 py-2 text-bone w-full"
                    on:change=move |ev| {
                        let v: u8 = event_target_value(&ev).parse().unwrap_or(2);
                        set_tier.set(v);
                    }
                >
                    <option value="1">"Tier 1 — Password only"</option>
                    <option value="2" selected=true>"Tier 2 — Password + Key file"</option>
                </select>
            </div>

            <Show when=move || tier.get() == 2>
                <div class="mb-4">
                    <label class="text-sm text-text-secondary block mb-1">"Key File Destination"</label>
                    <div class="flex gap-2 items-center">
                        <span class="text-sm text-bone flex-1">
                            {move || key_file_destination.get().clone().unwrap_or_else(|| "Not selected".to_string())}
                        </span>
                        <Button variant="secondary" on_click={
                            let on_browse_key = on_browse_key.clone();
                            move |ev| on_browse_key(ev)
                        }>"Browse"</Button>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Storage step — provider selection and configuration.
#[component]
pub fn StorageStep(
    _storage_provider: ReadSignal<StorageProvider>,
    _set_storage_provider: WriteSignal<StorageProvider>,
) -> impl IntoView {
    view! {
        <div class="space-y-4">
            <p class="text-sm text-text-secondary mb-4">
                "Choose where your vault data will be stored. You can change this later from settings."
            </p>
            <crate::components::StorageSelector />
        </div>
    }
}

/// Review step — summary of choices before creating vault.
#[component]
pub fn ReviewStep(
    vault_name: ReadSignal<String>,
    tier: ReadSignal<u8>,
    key_file_destination: ReadSignal<Option<String>>,
    storage_provider: ReadSignal<StorageProvider>,
) -> impl IntoView {
    view! {
        <div class="space-y-4">
            <div class="bg-surface-overlay border border-border-default rounded-lg p-4 space-y-3">
                <div class="flex justify-between items-start">
                    <span class="text-sm text-text-secondary">"Vault Name:"</span>
                    <span class="text-sm text-bone font-medium">{move || vault_name.get()}</span>
                </div>

                <div class="flex justify-between items-start">
                    <span class="text-sm text-text-secondary">"Authentication Tier:"</span>
                    <span class="text-sm text-bone font-medium">
                        {move || if tier.get() == 2 { "Tier 2 (Password + Key)" } else { "Tier 1 (Password only)" }}
                    </span>
                </div>

                {move || key_file_destination.get().map(|kfd| view! {
                    <div class="flex justify-between items-start">
                        <span class="text-sm text-text-secondary">"Key File:"</span>
                        <span class="text-xs text-bone font-mono">{kfd}</span>
                    </div>
                })}

                <div class="flex justify-between items-start">
                    <span class="text-sm text-text-secondary">"Storage:"</span>
                    <span class="text-sm text-bone font-medium">
                        {move || format!("{:?}", storage_provider.get())}
                    </span>
                </div>
            </div>

            <div class="bg-rune/10 border border-rune rounded-lg p-4">
                <p class="text-sm text-bone">
                    "Your vault will be created with the settings above. You can add additional storage destinations after creation."
                </p>
            </div>
        </div>
    }
}
