//! Ulanzi Dial — configurador GTK4/libadwaita para el D100H.

mod backend;
mod config;
mod ui;

use adw::prelude::*;
use libadwaita as adw;

const APP_ID: &str = "dev.ulanzi.dial";

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ulanzi_dial=info".into()),
        )
        .init();

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(ui::build_ui);
    app.run();
}
