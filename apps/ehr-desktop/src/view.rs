//! Session start and list reloading.
use crate::AppWindow;
use crate::models::{Session, State};
use crate::utils::{doc_row, documents_label, people_rows, selected_name, status};
use ehr_core::utils::date_text;
use ehr_core::{Category, Contact, ContactKind, EhrApp, Role};
use ipfs_network::NetworkConfig;
use slint::{ModelRc, VecModel, Weak};
use std::sync::Arc;

pub(crate) fn start_session(state: &Arc<State>, ui: &Weak<AppWindow>, app: EhrApp) {
    let app = Arc::new(app);
    let (state, ui) = (state.clone(), ui.clone());
    let runtime = state.runtime.handle().clone();
    runtime.spawn(async move {
        let listen = match NetworkConfig::listen_from_env() {
            Ok(listen) => listen,
            Err(error) => return status(&ui, error.to_string()),
        };
        match app.clone().start_node(listen, Vec::new(), false).await {
            Ok((node, _)) => {
                let name = app.profile().name.clone();
                *state
                    .session
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some(Arc::new(Session { app, node }));
                let _ = ui.upgrade_in_event_loop(move |window| {
                    window.set_me_name(name.into());
                    window.set_status("".into());
                    window.set_screen("main".into());
                });
                reload(&state, &ui);
            }
            Err(error) => status(&ui, format!("Nie udało się uruchomić węzła: {error}")),
        }
    });
}

/// Reads the lists from the databases (blocking thread), then updates the models in the window thread.
pub(crate) fn reload(state: &Arc<State>, ui: &Weak<AppWindow>) {
    let Some(session) = state.session() else {
        return;
    };
    let (search, category) = match ui.upgrade() {
        Some(window) => (
            window.get_search().to_lowercase(),
            window.get_category_filter(),
        ),
        None => return,
    };
    let (state, ui) = (state.clone(), ui.clone());
    state.runtime.spawn_blocking({
        let state = state.clone();
        move || {
            let app = &session.app;
            let result = (|| -> Result<_, ehr_core::EhrError> {
                let selected = state.view().selected_person.clone();
                Ok(match app.role() {
                    Role::Doctor => {
                        let people: Vec<Contact> = app
                            .contacts(ContactKind::Patient)?
                            .into_iter()
                            .filter(|contact| {
                                contact
                                    .person
                                    .display_name()
                                    .to_lowercase()
                                    .contains(&search)
                            })
                            .collect();
                        let pending = app.contacts(ContactKind::Pending)?;
                        let docs = match &selected {
                            Some(did) => app.documents(Some(did))?,
                            None => Vec::new(),
                        };
                        (people, pending, docs)
                    }
                    Role::Patient => {
                        let docs = app
                            .documents(None)?
                            .into_iter()
                            .filter(|entry| {
                                category < 0
                                    || Category::ALL.get(category as usize) == Some(&entry.category)
                            })
                            .filter(|entry| entry.file_name.to_lowercase().contains(&search))
                            .collect();
                        (app.contacts(ContactKind::Doctor)?, Vec::new(), docs)
                    }
                })
            })();
            let (people, pending, docs) = match result {
                Ok(lists) => lists,
                Err(error) => return status(&ui, format!("Błąd odczytu: {error}")),
            };
            let header = match app.role() {
                Role::Doctor => selected_name(&people, state.view().selected_person.as_deref()),
                Role::Patient => "Moje dokumenty".into(),
            };
            let note = match (app.role(), docs.first()) {
                (Role::Patient, _) => format!("tylko odczyt · {}", documents_label(docs.len())),
                (Role::Doctor, Some(last)) => {
                    format!(
                        "{} · zmiana {}",
                        documents_label(docs.len()),
                        date_text(last.created_utc)
                    )
                }
                (Role::Doctor, None) => String::new(),
            };
            let rows: Vec<_> = docs.iter().map(|entry| doc_row(app, entry)).collect();
            let mut view = state.view();
            let person_index = view
                .selected_person
                .as_ref()
                .and_then(|did| people.iter().position(|contact| &contact.did == did));
            let doc_index = view
                .selected_doc
                .as_ref()
                .and_then(|cid| docs.iter().position(|entry| &entry.manifest_cid == cid));
            let (people_model, pending_model) = (people_rows(&people), people_rows(&pending));
            view.people = people;
            view.pending = pending;
            view.docs = docs;
            drop(view);
            let _ = ui.upgrade_in_event_loop(move |window| {
                window.set_people(ModelRc::new(VecModel::from(people_model)));
                window.set_pending(ModelRc::new(VecModel::from(pending_model)));
                window.set_docs(ModelRc::new(VecModel::from(rows)));
                window.set_selected_person(person_index.map_or(-1, |index| index as i32));
                window.set_selected_doc(doc_index.map_or(-1, |index| index as i32));
                window.set_header(header.into());
                window.set_header_note(note.into());
            });
        }
    });
}
