//! EHR desktop app (Slint). Same logic as the CLI: everything goes through `ehr-core`
//! (profile, contacts, index, node). The role comes from the binary; the window and the CLI
//! with the same data folder cannot run at the same time (one LevelDB per process).
slint::include_modules!();

mod consts;
mod contacts;
mod documents;
mod models;
mod profile;
pub mod utils;
mod view;

use consts::REFRESH_SECS;
use ehr_core::{Category, EhrApp, Role};
use ipfs_network::NetworkCommand;
use models::{State, View};
use slint::ComponentHandle;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use utils::strings;

pub fn run(role: Role) -> Result<(), Box<dyn Error>> {
    let window = AppWindow::new()?;
    let data_dir = EhrApp::default_data_dir(role);
    std::fs::create_dir_all(&data_dir)?;
    let state = Arc::new(State {
        role,
        runtime: tokio::runtime::Runtime::new()?,
        data_dir: data_dir.clone(),
        session: Mutex::new(None),
        view: Mutex::new(View::default()),
    });
    window.set_is_doctor(role == Role::Doctor);
    window.set_categories(strings(
        Category::ALL
            .iter()
            .map(|category| category.label().to_owned()),
    ));
    match EhrApp::profile_name(&data_dir) {
        Some(name) if EhrApp::profile_exists(&data_dir) => {
            window.set_me_name(name.into());
            window.set_screen("login".into());
        }
        _ => window.set_screen("first".into()),
    }
    profile::register(&window, &state);
    contacts::register(&window, &state);
    documents::register(&window, &state);

    // New documents (inbox every 30 s in the node) and contact requests show up by themselves.
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(REFRESH_SECS),
        {
            let (state, ui) = (state.clone(), window.as_weak());
            move || view::reload(&state, &ui)
        },
    );

    window.run()?;
    if let Some(session) = state.session() {
        let _ = session.node.commands().send(NetworkCommand::Quit);
    }
    Ok(())
}
