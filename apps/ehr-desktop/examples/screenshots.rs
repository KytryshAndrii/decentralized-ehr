//! Narzędzie deweloperskie: renderuje ekrany UI (dane syntetyczne) do PNG bez okna,
//! rendererem programowym Slint. `cargo run -p ehr-desktop --example screenshots -- <katalog>`
use ehr_desktop::{AppWindow, DocRow, PersonRow};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, ModelRc, PhysicalSize, Rgb8Pixel, SharedString, VecModel};
use std::rc::Rc;

struct Headless(Rc<MinimalSoftwareWindow>);

type Shot = Box<dyn Fn(&AppWindow)>;

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn doc(title: &str, subtitle: &str, date: &str, category: &str) -> DocRow {
    DocRow {
        title: title.into(),
        subtitle: subtitle.into(),
        date: date.into(),
        category: category.into(),
        issuer: "dr Anna Malinowska".into(),
        size: "218 KB".into(),
        valid_until: "14.11.2046".into(),
        version: "1".into(),
    }
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let (width, height) = (1240u32, 720u32);
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(PhysicalSize::new(width, height));
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    let categories: Vec<SharedString> = ["Wyniki badań", "Obrazowe", "Wypisy i zalecenia", "Inne"]
        .map(Into::into)
        .to_vec();
    ui.set_categories(ModelRc::new(VecModel::from(categories)));
    let person = |name: &str| PersonRow {
        name: name.into(),
        did: "did:key:z6Mk…".into(),
    };
    let shots: Vec<(&str, Shot)> = vec![
        (
            "1a-first",
            Box::new(|ui: &AppWindow| {
                ui.set_is_doctor(true);
                ui.set_screen("first".into());
            }),
        ),
        (
            "1b-login",
            Box::new(|ui: &AppWindow| {
                ui.set_screen("login".into());
                ui.set_me_name("dr Anna Malinowska".into());
            }),
        ),
        (
            "2-doctor",
            Box::new(move |ui: &AppWindow| {
                ui.set_screen("main".into());
                ui.set_people(ModelRc::new(VecModel::from(vec![
                    person("Jan Kowalski"),
                    person("Anna Nowak"),
                ])));
                ui.set_pending(ModelRc::new(VecModel::from(vec![person(
                    "Piotr Wiśniewski",
                )])));
                ui.set_selected_person(0);
                ui.set_header("Jan Kowalski".into());
                ui.set_header_note("3 dokumenty · zmiana 14.11.2026".into());
                ui.set_docs(ModelRc::new(VecModel::from(vec![
                    doc(
                        "Morfologia krwi.pdf",
                        "218 KB · Wyniki badań",
                        "14.11.2026",
                        "Wyniki badań",
                    ),
                    doc(
                        "RTG klatki piersiowej.png",
                        "1,4 MB · Obrazowe",
                        "10.11.2026",
                        "Obrazowe",
                    ),
                    doc(
                        "Wypis ze szpitala.pdf",
                        "96 KB · Wypisy i zalecenia",
                        "02.11.2026",
                        "Wypisy i zalecenia",
                    ),
                ])));
                ui.set_selected_doc(0);
                ui.set_preview_ready(true);
                ui.set_doc_verified(true);
                ui.set_preview_text(
                    "Podgląd niedostępny dla tego typu pliku. Użyj „Pobierz”.".into(),
                );
            }),
        ),
        (
            "3-share",
            Box::new(|ui: &AppWindow| {
                ui.set_modal("add".into());
                ui.set_file_path("/home/lekarz/wyniki/morfologia.pdf".into());
                ui.set_file_name("Morfologia krwi.pdf".into());
                ui.set_recipient("Jan Kowalski".into());
                ui.set_valid_until("18.11.2046".into());
            }),
        ),
        (
            "4-patient",
            Box::new(move |ui: &AppWindow| {
                ui.set_modal("none".into());
                ui.set_is_doctor(false);
                ui.set_me_name("Jan Kowalski".into());
                ui.set_people(ModelRc::new(VecModel::from(vec![person(
                    "dr Anna Malinowska",
                )])));
                ui.set_header("Moje dokumenty".into());
                ui.set_header_note("tylko odczyt · 2 dokumenty".into());
                ui.set_docs(ModelRc::new(VecModel::from(vec![
                    doc(
                        "Morfologia krwi.pdf",
                        "dr Anna Malinowska · 218 KB",
                        "14.11.2026",
                        "Wyniki badań",
                    ),
                    doc(
                        "Zalecenia po zabiegu.txt",
                        "dr Anna Malinowska · 41 KB",
                        "28.10.2026",
                        "Wypisy i zalecenia",
                    ),
                ])));
                ui.set_selected_doc(1);
                ui.set_preview_text("Zalecenia: odpoczynek 3 dni, kontrola za 2 tygodnie.".into());
            }),
        ),
        (
            "5-connect",
            Box::new(|ui: &AppWindow| {
                ui.set_modal("connect".into());
                ui.set_connect_text("ehr1:eyJ2IjoxLCJzaWduZXIiOiJkaWQ6a2V5Ono2TWt…".into());
            }),
        ),
    ];
    ui.show().unwrap();
    for (name, setup) in shots {
        setup(&ui);
        window.request_redraw();
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            let mut pixels = vec![Rgb8Pixel::default(); (width * height) as usize];
            renderer.render(&mut pixels, width as usize);
            let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
            image::RgbImage::from_raw(width, height, bytes)
                .unwrap()
                .save(out.join(format!("{name}.png")))
                .unwrap();
        });
        println!("{name}.png");
    }
}
