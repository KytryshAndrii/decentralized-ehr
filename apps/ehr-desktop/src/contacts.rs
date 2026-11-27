//! People list, filters, refresh, invitations and contact requests.
use crate::AppWindow;
use crate::models::State;
use crate::utils::status;
use crate::view::reload;
use ehr_core::{EhrApp, Role};
use ipfs_network::NetworkConfig;
use slint::ComponentHandle;
use std::sync::Arc;

pub(crate) fn register(window: &AppWindow, state: &Arc<State>) {
    let ui = window.as_weak();

    window.on_select_person({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let did = usize::try_from(index).ok().and_then(|index| {
                state
                    .view()
                    .people
                    .get(index)
                    .map(|contact| contact.did.clone())
            });
            let mut view = state.view();
            view.selected_person = did;
            view.selected_doc = None;
            drop(view);
            reload(&state, &ui);
        }
    });

    window.on_filter_changed({
        let (state, ui) = (state.clone(), ui.clone());
        move || reload(&state, &ui)
    });

    window.on_refresh({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let Some(session) = state.session() else {
                return;
            };
            if state.role == Role::Patient {
                status(&ui, "Sprawdzanie nowych dokumentów…");
                let (task_state, task_ui) = (state.clone(), ui.clone());
                state.runtime.spawn(async move {
                    match session.node.sync_inbox().await {
                        Ok(received) => {
                            status(&task_ui, format!("Nowe dokumenty: {}", received.len()))
                        }
                        Err(error) => status(&task_ui, format!("Błąd: {error}")),
                    }
                    reload(&task_state, &task_ui);
                });
            } else {
                reload(&state, &ui);
            }
        }
    });

    window.on_new_invite({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let (Some(session), Some(window)) = (state.session(), ui.upgrade()) else {
                return;
            };
            let invite = NetworkConfig::listen_from_env()
                .map_err(|error| error.to_string())
                .and_then(|listen| EhrApp::node_address(&listen).map_err(|error| error.to_string()))
                .and_then(|address| {
                    session
                        .app
                        .create_invite(&address)
                        .map_err(|error| error.to_string())
                });
            match invite {
                Ok(text) => {
                    window.set_invite_text(text.into());
                    window.set_modal("invite".into());
                }
                Err(error) => window.set_status(format!("Zaproszenie: {error}").into()),
            }
        }
    });

    window.on_connect_doctor({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let (Some(session), Some(window)) = (state.session(), ui.upgrade()) else {
                return;
            };
            let text = window.get_connect_text().trim().to_owned();
            window.set_status("Łączenie z lekarzem…".into());
            let (task_state, task_ui) = (state.clone(), ui.clone());
            state.runtime.spawn(async move {
                match session.node.connect_doctor(&text).await {
                    Ok(doctor) => {
                        let _ =
                            task_ui.upgrade_in_event_loop(|window| window.set_modal("none".into()));
                        status(
                            &task_ui,
                            format!(
                                "Dodano lekarza {}. Dokumenty pojawią się po akceptacji.",
                                doctor.person.display_name()
                            ),
                        );
                        reload(&task_state, &task_ui);
                    }
                    Err(error) => status(&task_ui, format!("Nie połączono: {error}")),
                }
            });
        }
    });

    window.on_open_accept({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let Some(window) = ui.upgrade() else { return };
            let contact = usize::try_from(index)
                .ok()
                .and_then(|index| state.view().pending.get(index).cloned());
            if let Some(contact) = contact {
                window.set_accept_did(contact.did.clone().into());
                window.set_accept_name(contact.person.display_name().into());
                window.set_modal("accept".into());
            }
        }
    });

    window.on_accept_contact({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let (Some(session), Some(window)) = (state.session(), ui.upgrade()) else {
                return;
            };
            match session
                .app
                .accept_contact(&window.get_accept_did(), Some(&window.get_accept_name()))
            {
                Ok(contact) => {
                    window.set_modal("none".into());
                    window.set_status(
                        format!("Dodano pacjenta {}", contact.person.display_name()).into(),
                    );
                    reload(&state, &ui);
                }
                Err(error) => window.set_status(format!("Błąd: {error}").into()),
            }
        }
    });

    window.on_reject_contact({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let (Some(session), Some(window)) = (state.session(), ui.upgrade()) else {
                return;
            };
            let _ = session.app.reject_contact(&window.get_accept_did());
            window.set_modal("none".into());
            reload(&state, &ui);
        }
    });
}
