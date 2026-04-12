mod app;

use app::App;
use leptos::mount;
use log::Level;

/// Entrypoint for the Arx Runa Leptos frontend.
fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(Level::Debug).ok();
    mount::mount_to_body(App);
}
