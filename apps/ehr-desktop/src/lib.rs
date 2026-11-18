//! Aplikacja okienkowa EHR (Slint). Ta sama logika co CLI: wszystko idzie przez `ehr-core`
//! (profil, kontakty, rejestr, węzeł). Rola pochodzi z binarki; okno i CLI z tym samym
//! katalogiem danych nie działają jednocześnie (jedna baza LevelDB na proces).
slint::include_modules!();

use ehr_core::utils::{add_years, date_text, media_type_for, size_text, utc_now_secs};
use ehr_core::{Category, Contact, ContactKind, EhrApp, EhrNode, IndexEntry, NewDocument, Role};
use ipfs_network::{NetworkCommand, NetworkConfig};
use slint::{ComponentHandle, ModelRc, SharedPixelBuffer, SharedString, VecModel, Weak};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REFRESH_SECS: u64 = 3;
const PREVIEW_TEXT_CHARS: usize = 2000;

struct Session {
    app: Arc<EhrApp>,
    node: EhrNode,
}

/// Stan współdzielony przez wywołania zwrotne okna.
#[derive(Default)]
struct View {
    people: Vec<Contact>,
    pending: Vec<Contact>,
    docs: Vec<IndexEntry>,
    selected_person: Option<String>,
    selected_doc: Option<String>,
}

struct State {
    role: Role,
    runtime: tokio::runtime::Runtime,
    data_dir: PathBuf,
    session: Mutex<Option<Arc<Session>>>,
    view: Mutex<View>,
}

impl State {
    fn session(&self) -> Option<Arc<Session>> {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn view(&self) -> std::sync::MutexGuard<'_, View> {
        self.view.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn status(ui: &Weak<AppWindow>, text: impl Into<String>) {
    let text = text.into();
    let _ = ui.upgrade_in_event_loop(move |ui| ui.set_status(text.into()));
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(Into::into)
            .collect::<Vec<SharedString>>(),
    ))
}

fn people_rows(contacts: &[Contact]) -> Vec<PersonRow> {
    contacts
        .iter()
        .map(|contact| PersonRow {
            name: contact.person.display_name().into(),
            did: contact.did.clone().into(),
        })
        .collect()
}

fn doc_row(app: &EhrApp, entry: &IndexEntry) -> DocRow {
    let issuer = app.display_name(&entry.issuer_did);
    DocRow {
        title: entry.file_name.clone().into(),
        subtitle: match app.role() {
            Role::Doctor => format!("{} · {}", size_text(entry.size), entry.category.label()),
            Role::Patient => format!("{issuer} · {}", size_text(entry.size)),
        }
        .into(),
        date: date_text(entry.created_utc).into(),
        category: entry.category.label().into(),
        issuer: issuer.into(),
        size: size_text(entry.size).into(),
        valid_until: date_text(entry.valid_until_utc).into(),
        version: entry.version.to_string().into(),
    }
}

/// Odczyt list z baz (wątek blokujący), potem aktualizacja modeli w wątku okna.
fn reload(state: &Arc<State>, ui: &Weak<AppWindow>) {
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
                            .filter(|c| c.person.display_name().to_lowercase().contains(&search))
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
            let rows: Vec<DocRow> = docs.iter().map(|entry| doc_row(app, entry)).collect();
            let mut view = state.view();
            let person_index = view
                .selected_person
                .as_ref()
                .and_then(|did| people.iter().position(|c| &c.did == did));
            let doc_index = view
                .selected_doc
                .as_ref()
                .and_then(|cid| docs.iter().position(|d| &d.manifest_cid == cid));
            let (people_model, pending_model) = (people_rows(&people), people_rows(&pending));
            view.people = people;
            view.pending = pending;
            view.docs = docs;
            drop(view);
            let _ = ui.upgrade_in_event_loop(move |window| {
                window.set_people(ModelRc::new(VecModel::from(people_model)));
                window.set_pending(ModelRc::new(VecModel::from(pending_model)));
                window.set_docs(ModelRc::new(VecModel::from(rows)));
                window.set_selected_person(person_index.map_or(-1, |i| i as i32));
                window.set_selected_doc(doc_index.map_or(-1, |i| i as i32));
                window.set_header(header.into());
                window.set_header_note(note.into());
            });
        }
    });
}

/// Polska odmiana: 1 dokument, 2–4 dokumenty, 5+ dokumentów (12–14: dokumentów).
fn documents_label(count: usize) -> String {
    let word = match (count % 10, count % 100) {
        _ if count == 1 => "dokument",
        (2..=4, tens) if !(12..=14).contains(&tens) => "dokumenty",
        _ => "dokumentów",
    };
    format!("{count} {word}")
}

fn selected_name(people: &[Contact], did: Option<&str>) -> String {
    did.and_then(|did| people.iter().find(|c| c.did == did))
        .map_or_else(
            || "Wybierz pacjenta".into(),
            |c| c.person.display_name().to_owned(),
        )
}

fn start_session(state: &Arc<State>, ui: &Weak<AppWindow>, app: EhrApp) {
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
                *state.session.lock().unwrap_or_else(|e| e.into_inner()) =
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

/// Katalog pobranych plików; nazwa od wystawcy jest oczyszczona i nie nadpisuje istniejących.
fn download_path(file_name: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ".".into());
    let directory = ["Pobrane", "Downloads"]
        .iter()
        .map(|name| home.join(name))
        .find(|path| path.is_dir())
        .unwrap_or(home);
    let name = ehr_core::utils::safe_file_name(file_name);
    let mut path = directory.join(&name);
    let mut copy = 1;
    while path.exists() {
        path = directory.join(format!("({copy}) {name}"));
        copy += 1;
    }
    path
}

fn preview(
    entry: &IndexEntry,
    bytes: &[u8],
) -> Result<SharedPixelBuffer<slint::Rgba8Pixel>, String> {
    match entry.media_type.as_str() {
        "image/png" | "image/jpeg" => {
            let image = image::load_from_memory(bytes)
                .map_err(|_| "Nie można wyświetlić obrazu.".to_string())?
                .to_rgba8();
            Ok(SharedPixelBuffer::clone_from_slice(
                image.as_raw(),
                image.width(),
                image.height(),
            ))
        }
        media if media.starts_with("text/") || media.contains("json") || media.contains("xml") => {
            Err(String::from_utf8_lossy(bytes)
                .chars()
                .take(PREVIEW_TEXT_CHARS)
                .collect())
        }
        _ => Err("Podgląd niedostępny dla tego typu pliku. Użyj „Pobierz”.".into()),
    }
}

fn doc_at(state: &State, index: i32) -> Option<IndexEntry> {
    usize::try_from(index)
        .ok()
        .and_then(|i| state.view().docs.get(i).cloned())
}

pub fn run(role: Role) -> Result<(), Box<dyn Error>> {
    let window = AppWindow::new()?;
    let data_dir = EhrApp::default_data_dir();
    let state = Arc::new(State {
        role,
        runtime: tokio::runtime::Runtime::new()?,
        data_dir: data_dir.clone(),
        session: Mutex::new(None),
        view: Mutex::new(View::default()),
    });
    window.set_is_doctor(role == Role::Doctor);
    window.set_categories(strings(Category::ALL.iter().map(|c| c.label().to_owned())));
    match EhrApp::profile_name(&data_dir) {
        Some(name) if EhrApp::profile_exists(&data_dir) => {
            window.set_me_name(name.into());
            window.set_screen("login".into());
        }
        _ => window.set_screen("first".into()),
    }
    let ui = window.as_weak();

    window.on_create_profile({
        let (state, ui) = (state.clone(), ui.clone());
        move |name, password, repeated| {
            if password != repeated {
                return status(&ui, "Hasła się różnią.");
            }
            status(&ui, "Tworzenie kluczy…");
            let (state2, ui2) = (state.clone(), ui.clone());
            state.runtime.spawn_blocking(move || {
                let _ = std::fs::create_dir_all(&state2.data_dir);
                match EhrApp::create_profile(&state2.data_dir, state2.role, &name, &password) {
                    Ok(app) => start_session(&state2, &ui2, app),
                    Err(error) => status(&ui2, format!("Nie utworzono profilu: {error}")),
                }
            });
        }
    });

    window.on_login({
        let (state, ui) = (state.clone(), ui.clone());
        move |password| {
            status(&ui, "Odblokowywanie kluczy…");
            let (state2, ui2) = (state.clone(), ui.clone());
            state.runtime.spawn_blocking(move || {
                match EhrApp::open(&state2.data_dir, state2.role, &password) {
                    Ok(app) => start_session(&state2, &ui2, app),
                    Err(error) => status(&ui2, format!("Logowanie nieudane: {error}")),
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
                .unwrap_or_else(|e| e.into_inner())
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

    window.on_select_person({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let did = usize::try_from(index)
                .ok()
                .and_then(|i| state.view().people.get(i).map(|c| c.did.clone()));
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
                let (state2, ui2) = (state.clone(), ui.clone());
                state.runtime.spawn(async move {
                    match session.node.sync_inbox().await {
                        Ok(new) => status(&ui2, format!("Nowe dokumenty: {}", new.len())),
                        Err(error) => status(&ui2, format!("Błąd: {error}")),
                    }
                    reload(&state2, &ui2);
                });
            } else {
                reload(&state, &ui);
            }
        }
    });

    window.on_select_doc({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let (Some(session), Some(entry)) = (state.session(), doc_at(&state, index)) else {
                return;
            };
            state.view().selected_doc = Some(entry.manifest_cid.clone());
            if let Some(window) = ui.upgrade() {
                window.set_selected_doc(index);
                window.set_preview_ready(false);
                window.set_has_preview_image(false);
                window.set_doc_verified(false);
            }
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
    });

    window.on_download({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let (Some(session), Some(entry)) = (state.session(), doc_at(&state, index)) else {
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
                    return;
                };
                let path = file.path().to_path_buf();
                let _ = ui.upgrade_in_event_loop(move |window| {
                    // Przy edycji nazwa dokumentu zostaje, przy dodawaniu bierzemy nazwę pliku.
                    if window.get_modal() == "add" {
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
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
            window.set_valid_until(date_text(add_years(utc_now_secs(), 20)).into());
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
            let typed = window.get_file_name().trim().to_owned();
            let file_name = if typed.is_empty() {
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                typed
            };
            let category = Category::ALL
                .get(window.get_file_category().max(0) as usize)
                .copied()
                .unwrap_or(Category::Other);
            let recipient = window.get_recipient().trim().to_owned();
            // Zaznaczony pacjent po DID: dwie osoby o tym samym nazwisku nie blokują wysyłki.
            let selected = {
                let view = state.view();
                view.selected_person.as_ref().and_then(|did| {
                    view.people
                        .iter()
                        .find(|c| &c.did == did && c.person.display_name() == recipient)
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
            let (state2, ui2) = (state.clone(), ui.clone());
            state.runtime.spawn(async move {
                let document = NewDocument {
                    media_type: media_type_for(&file_name).into(),
                    path,
                    patient_did: patient.did.clone(),
                    file_name,
                    category,
                };
                match session.node.issue_document(document).await {
                    Ok(entry) => {
                        state2.view().selected_person = Some(patient.did);
                        let _ = ui2.upgrade_in_event_loop(|w| w.set_modal("none".into()));
                        status(&ui2, format!("Udostępniono: {}", entry.file_name));
                        reload(&state2, &ui2);
                    }
                    Err(error) => status(&ui2, format!("Nie udostępniono: {error}")),
                }
            });
        }
    });

    window.on_open_edit({
        let (state, ui) = (state.clone(), ui.clone());
        move |index| {
            let (Some(entry), Some(window)) = (doc_at(&state, index), ui.upgrade()) else {
                return;
            };
            state.view().selected_doc = Some(entry.manifest_cid.clone());
            window.set_selected_doc(index);
            window.set_file_name(entry.file_name.into());
            window.set_file_path("".into());
            window.set_valid_until(date_text(add_years(utc_now_secs(), 20)).into());
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
                view.selected_doc
                    .as_ref()
                    .and_then(|cid| view.docs.iter().find(|d| &d.manifest_cid == cid).cloned())
            };
            let Some(entry) = entry else { return };
            let path = PathBuf::from(window.get_file_path().as_str());
            if !path.is_absolute() || !path.is_file() {
                return window.set_status("Wybierz istniejący plik (pełna ścieżka).".into());
            }
            window.set_status("Zapisywanie nowej wersji…".into());
            let (state2, ui2) = (state.clone(), ui.clone());
            state.runtime.spawn(async move {
                match session.node.update_document(&entry, Path::new(&path)).await {
                    Ok(new) => {
                        state2.view().selected_doc = Some(new.manifest_cid.clone());
                        let _ = ui2.upgrade_in_event_loop(|w| w.set_modal("none".into()));
                        status(
                            &ui2,
                            format!("Zapisano wersję {} pliku {}", new.version, new.file_name),
                        );
                        reload(&state2, &ui2);
                    }
                    Err(error) => status(&ui2, format!("Nie zapisano: {error}")),
                }
            });
        }
    });

    window.on_new_invite({
        let (state, ui) = (state.clone(), ui.clone());
        move || {
            let (Some(session), Some(window)) = (state.session(), ui.upgrade()) else {
                return;
            };
            let invite = NetworkConfig::listen_from_env()
                .map_err(|e| e.to_string())
                .and_then(|listen| EhrApp::node_address(&listen).map_err(|e| e.to_string()))
                .and_then(|address| {
                    session
                        .app
                        .create_invite(&address)
                        .map_err(|e| e.to_string())
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
            let (state2, ui2) = (state.clone(), ui.clone());
            state.runtime.spawn(async move {
                match session.node.connect_doctor(&text).await {
                    Ok(doctor) => {
                        let _ = ui2.upgrade_in_event_loop(|w| w.set_modal("none".into()));
                        status(
                            &ui2,
                            format!(
                                "Dodano lekarza {}. Dokumenty pojawią się po akceptacji.",
                                doctor.person.display_name()
                            ),
                        );
                        reload(&state2, &ui2);
                    }
                    Err(error) => status(&ui2, format!("Nie połączono: {error}")),
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
                .and_then(|i| state.view().pending.get(i).cloned());
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

    // Nowe dokumenty (skrzynka co 30 s w węźle) i prośby o kontakt pojawiają się same.
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(REFRESH_SECS),
        {
            let (state, ui) = (state.clone(), ui.clone());
            move || reload(&state, &ui)
        },
    );

    window.run()?;
    if let Some(session) = state.session() {
        let _ = session.node.commands().send(NetworkCommand::Quit);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn documents_label_polish_plural() {
        let labels: Vec<String> = [0, 1, 2, 4, 5, 12, 14, 21, 22, 112]
            .map(super::documents_label)
            .to_vec();
        assert_eq!(
            labels,
            [
                "0 dokumentów",
                "1 dokument",
                "2 dokumenty",
                "4 dokumenty",
                "5 dokumentów",
                "12 dokumentów",
                "14 dokumentów",
                "21 dokumentów",
                "22 dokumenty",
                "112 dokumentów"
            ]
        );
    }
}
