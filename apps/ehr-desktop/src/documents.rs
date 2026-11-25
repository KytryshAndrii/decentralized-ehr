//! Document preview, download, issuing and new versions.
use crate::AppWindow;
use crate::models::{Session, State};
use crate::utils::{download_path, preview, selected_name, status};
use crate::view::reload;
use ehr_core::consts::DOCUMENT_VALIDITY_YEARS;
use ehr_core::utils::{add_years, date_text, utc_now_secs};
use ehr_core::{Category, IndexEntry, NewDocument};
use slint::{ComponentHandle, Weak};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn register(window: &AppWindow, state: &Arc<State>) {
    let ui = window.as_weak();

    window.on_select_doc({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let (Some(session), Some(entry)) = (state.session(), state.doc_at(index)) else {
                return;
            };
            state.view().selected_doc = Some(entry.manifest_cid.clone());
            if let Some(window) = ui.upgrade() {
                window.set_selected_doc(index);
            }
            show_preview(&state, &ui, session, entry);
        }
    });

    window.on_download({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let (Some(session), Some(entry)) = (state.session(), state.doc_at(index)) else {
                return;
            };
            let ui = ui.clone();
            state.runtime.spawn(async move {
                let path = download_path(&entry.file_name);
                match session.node.save_document(&entry, &path).await {
                    Ok(()) => status(&ui, format!("Zapisano: {}", path.display())),
                    Err(error) => status(&ui, format!("Nie zapisano: {error}")),
                }
            });
        }
    });

    window.on_pick_file({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let ui = ui.clone();
            state.runtime.spawn(async move {
                let Some(file) = rfd::AsyncFileDialog::new()
                    .set_title("Wybierz plik")
                    .pick_file()
                    .await
                else {
                    // rfd: without xdg-desktop-portal (e.g. WSL) it needs zenity, otherwise it returns None silently.
                    return status(
                        &ui,
                        "Nie wybrano pliku. Jeśli okno wyboru się nie otworzyło, zainstaluj zenity \
                         (sudo apt install zenity) albo wpisz pełną ścieżkę w polu „Plik”.",
                    );
                };
                let path = file.path().to_path_buf();
                let _ = ui.upgrade_in_event_loop(move |window| {
                    // Editing keeps the document name, adding takes the file name.
                    if window.get_modal() == "add" {
                        let name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned());
                        window.set_file_name(name.unwrap_or_default().into());
                    }
                    window.set_file_path(path.display().to_string().into());
                });
            });
        }
    });

    window.on_open_add({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let Some(window) = ui.upgrade() else { return };
            let view = state.view();
            let name = selected_name(&view.people, view.selected_person.as_deref());
            window.set_recipient(name.into());
            window.set_file_path("".into());
            window.set_file_name("".into());
            window.set_file_category(0);
            window.set_valid_until(
                date_text(add_years(utc_now_secs(), DOCUMENT_VALIDITY_YEARS)).into(),
            );
            window.set_status("".into());
            window.set_modal("add".into());
        }
    });

    window.on_confirm_add({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let (Some(session), Some(window)) = (state.session(), ui.upgrade()) else {
                return;
            };
            let path = PathBuf::from(window.get_file_path().as_str());
            let file_name = window.get_file_name().to_string();
            let category = Category::ALL
                .get(window.get_file_category().max(0) as usize)
                .copied()
                .unwrap_or(Category::Other);
            let recipient = window.get_recipient().trim().to_owned();
            // The selected patient by DID: two people with the same name do not block sending.
            let selected = {
                let view = state.view();
                view.selected_person.as_ref().and_then(|did| {
                    view.people
                        .iter()
                        .find(|contact| {
                            &contact.did == did && contact.person.display_name() == recipient
                        })
                        .cloned()
                })
            };
            let patient = match selected.map_or_else(|| session.app.find_patient(&recipient), Ok) {
                Ok(patient) => patient,
                Err(error) => return window.set_status(format!("Odbiorca: {error}").into()),
            };
            if !path.is_absolute() || !path.is_file() {
                return window.set_status("Wybierz istniejący plik (pełna ścieżka).".into());
            }
            window.set_status("Szyfrowanie i udostępnianie…".into());
            let (task_state, task_ui) = (state.clone(), ui.clone());
            state.runtime.spawn(async move {
                let document = NewDocument {
                    path,
                    patient_did: patient.did.clone(),
                    file_name,
                    category,
                };
                match session.node.issue_document(document).await {
                    Ok(entry) => {
                        task_state.view().selected_person = Some(patient.did);
                        let _ =
                            task_ui.upgrade_in_event_loop(|window| window.set_modal("none".into()));
                        status(&task_ui, format!("Udostępniono: {}", entry.file_name));
                        reload(&task_state, &task_ui);
                    }
                    Err(error) => status(&task_ui, format!("Nie udostępniono: {error}")),
                }
            });
        }
    });

    window.on_open_edit({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let (Some(entry), Some(window)) = (state.doc_at(index), ui.upgrade()) else {
                return;
            };
            // Selecting shows the details panel, so its preview must start too.
            window.invoke_select_doc(index);
            window.set_file_name(entry.file_name.into());
            window.set_file_path("".into());
            window.set_valid_until(
                date_text(add_years(utc_now_secs(), DOCUMENT_VALIDITY_YEARS)).into(),
            );
            window.set_status("".into());
            window.set_modal("edit".into());
        }
    });

    window.on_confirm_edit({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let (Some(session), Some(window)) = (state.session(), ui.upgrade()) else {
                return;
            };
            let entry = {
                let view = state.view();
                view.selected_doc.as_ref().and_then(|cid| {
                    view.docs
                        .iter()
                        .find(|entry| &entry.manifest_cid == cid)
                        .cloned()
                })
            };
            let Some(entry) = entry else { return };
            let path = PathBuf::from(window.get_file_path().as_str());
            if !path.is_absolute() || !path.is_file() {
                return window.set_status("Wybierz istniejący plik (pełna ścieżka).".into());
            }
            window.set_status("Zapisywanie nowej wersji…".into());
            let (task_state, task_ui) = (state.clone(), ui.clone());
            state.runtime.spawn(async move {
                match session.node.update_document(&entry, &path).await {
                    Ok(updated) => {
                        task_state.view().selected_doc = Some(updated.manifest_cid.clone());
                        show_preview(&task_state, &task_ui, session.clone(), updated.clone());
                        let _ =
                            task_ui.upgrade_in_event_loop(|window| window.set_modal("none".into()));
                        status(
                            &task_ui,
                            format!(
                                "Zapisano wersję {} pliku {}",
                                updated.version, updated.file_name
                            ),
                        );
                        reload(&task_state, &task_ui);
                    }
                    Err(error) => status(&task_ui, format!("Nie zapisano: {error}")),
                }
            });
        }
    });
}

/// Decrypts and verifies the document in the background, then fills the details panel.
fn show_preview(
    state: &Arc<State>,
    ui: &Weak<AppWindow>,
    session: Arc<Session>,
    entry: IndexEntry,
) {
    let _ = ui.upgrade_in_event_loop(|window| {
        window.set_preview_ready(false);
        window.set_has_preview_image(false);
        window.set_doc_verified(false);
    });
    let ui = ui.clone();
    state.runtime.spawn(async move {
        let result = session.node.open_document(&entry).await;
        let _ = ui.upgrade_in_event_loop(move |window| {
            window.set_doc_verified(result.is_ok());
            match result.map_err(|error| format!("Nie można otworzyć: {error}")) {
                Ok(bytes) => match preview(&entry, &bytes) {
                    Ok(pixels) => {
                        window.set_preview_image(slint::Image::from_rgba8(pixels));
                        window.set_has_preview_image(true);
                    }
                    Err(text) => window.set_preview_text(text.into()),
                },
                Err(text) => window.set_preview_text(text.into()),
            }
            window.set_preview_ready(true);
        });
    });
}
