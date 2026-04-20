use leptos::prelude::*;

/// Root frontend component rendered in the Tauri webview.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <main class="container">
            <h1>"Hello Arx Runa"</h1>
        </main>
    }
}
