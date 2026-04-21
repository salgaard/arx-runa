use arx_runa::App;
use leptos::mount;
use log::Level;

/// Entrypoint for the Arx Runa Leptos frontend.
fn main() {
    console_error_panic_hook::set_once();
    #[cfg(debug_assertions)]
    console_log::init_with_level(Level::Debug).ok();
    #[cfg(not(debug_assertions))]
    console_log::init_with_level(Level::Warn).ok();
    mount::mount_to_body(App);
}
