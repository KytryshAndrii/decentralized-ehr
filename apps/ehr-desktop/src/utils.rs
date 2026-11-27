use crate::consts::{DOWNLOADS_DIR, PREVIEW_TEXT_CHARS};
use crate::{AppWindow, DocRow, PersonRow};
use ehr_core::utils::{date_text, size_text};
use ehr_core::{Contact, EhrApp, IndexEntry, Role};
use slint::{ModelRc, SharedPixelBuffer, SharedString, VecModel, Weak};
use std::path::PathBuf;

pub(crate) fn status(ui: &Weak<AppWindow>, text: impl Into<String>) {
    let text = text.into();
    let _ = ui.upgrade_in_event_loop(move |window| window.set_status(text.into()));
}

pub(crate) fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(Into::into)
            .collect::<Vec<SharedString>>(),
    ))
}

pub(crate) fn people_rows(contacts: &[Contact]) -> Vec<PersonRow> {
    contacts
        .iter()
        .map(|contact| PersonRow {
            name: contact.person.display_name().into(),
            did: contact.did.clone().into(),
        })
        .collect()
}

pub(crate) fn doc_row(app: &EhrApp, entry: &IndexEntry) -> DocRow {
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

/// Polish plural: 1 dokument, 2–4 dokumenty, 5+ dokumentów (12–14: dokumentów).
pub fn documents_label(count: usize) -> String {
    let word = match (count % 10, count % 100) {
        _ if count == 1 => "dokument",
        (2..=4, tens) if !(12..=14).contains(&tens) => "dokumenty",
        _ => "dokumentów",
    };
    format!("{count} {word}")
}

pub(crate) fn selected_name(people: &[Contact], did: Option<&str>) -> String {
    did.and_then(|did| people.iter().find(|contact| contact.did == did))
        .map_or_else(
            || "Wybierz pacjenta".into(),
            |contact| contact.person.display_name().to_owned(),
        )
}

/// `~/Downloads` (created when missing); the issuer's name is cleaned and never overwrites a file.
pub(crate) fn download_path(file_name: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ".".into());
    let directory = home.join(DOWNLOADS_DIR);
    let _ = std::fs::create_dir_all(&directory);
    let name = ehr_core::utils::safe_file_name(file_name);
    let mut path = directory.join(&name);
    let mut copy = 1;
    while path.exists() {
        path = directory.join(format!("({copy}) {name}"));
        copy += 1;
    }
    path
}

/// An image to show, or the text shown instead of it.
pub(crate) fn preview(
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
