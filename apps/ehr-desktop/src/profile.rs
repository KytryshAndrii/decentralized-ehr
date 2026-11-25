//! First run, login and logout.
use crate::AppWindow;
use crate::models::{State, View};
use crate::utils::status;
use crate::view::start_session;
use ehr_core::EhrApp;
use ipfs_network::NetworkCommand;
use slint::{ComponentHandle, ModelRc};
use std::sync::Arc;

pub(crate) fn register(window: &AppWindow, state: &Arc<State>) {
    let ui = window.as_weak();

    window.on_create_profile({
        let (state, ui) = (state.clone(), ui.clone());
        move |name, password, repeated| {
            if password != repeated {
                return status(&ui, "Hasła się różnią.");
            }
            status(&ui, "Tworzenie kluczy…");
            let (task_state, task_ui) = (state.clone(), ui.clone());
            state.runtime.spawn_blocking(move || {
                match EhrApp::create_profile(
                    &task_state.data_dir,
                    task_state.role,
                    &name,
                    &password,
                ) {
                    Ok(app) => start_session(&task_state, &task_ui, app),
                    Err(error) => status(&task_ui, format!("Nie utworzono profilu: {error}")),
                }
            });
        }
    });

    window.on_login({
        let (state, ui) = (state.clone(), ui.clone());
        move |password| {
            status(&ui, "Odblokowywanie kluczy…");
            let (task_state, task_ui) = (state.clone(), ui.clone());
            state.runtime.spawn_blocking(move || {
                match EhrApp::open(&task_state.data_dir, task_state.role, &password) {
                    Ok(app) => start_session(&task_state, &task_ui, app),
                    Err(error) => status(&task_ui, format!("Logowanie nieudane: {error}")),
                }
            });
        }
    });

    window.on_logout({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            if let Some(session) = state
                .session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
                let _ = session.node.commands().send(NetworkCommand::Quit);
            }
            *state.view() = View::default();
            if let Some(window) = ui.upgrade() {
                window.set_screen("login".into());
                window.set_status("".into());
                window.set_docs(ModelRc::default());
                window.set_people(ModelRc::default());
                window.set_pending(ModelRc::default());
            }
        }
    });
}
