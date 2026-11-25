//! Time is always UTC (D-26). Gregorian calendar without dependencies: the
//! `days_from_civil` / `civil_from_days` algorithm (H. Hinnant).
pub use ehr_security::peer_id_from_did;
pub use ipfs_store::utils::utc_now_secs;

const DAY_SECS: i64 = 86_400;

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

fn split(secs: u64) -> ((i64, i64, i64), i64) {
    let secs = secs as i64;
    (
        civil_from_days(secs.div_euclid(DAY_SECS)),
        secs.rem_euclid(DAY_SECS),
    )
}

/// The same day and hour `years` later; 29 February becomes 28 February.
pub fn add_years(secs: u64, years: i64) -> u64 {
    let ((year, month, day), time) = split(secs);
    let target = year + years;
    let leap = (target % 4 == 0 && target % 100 != 0) || target % 400 == 0;
    let day = if month == 2 && day == 29 && !leap {
        28
    } else {
        day
    };
    (days_from_civil(target, month, day) * DAY_SECS + time) as u64
}

/// `instant` / `dateTime` FHIR, np. `2026-11-18T09:12:00Z`.
pub fn iso8601(secs: u64) -> String {
    let ((year, month, day), time) = split(secs);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

/// Date to show, e.g. `18.11.2026`.
pub fn date_text(secs: u64) -> String {
    let ((year, month, day), _) = split(secs);
    format!("{day:02}.{month:02}.{year:04}")
}

/// Size to show (`218 KB`, `1,4 MB`).
pub fn size_text(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KB", bytes / 1024),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0).replace('.', ","),
    }
}

/// MIME type from the extension, for `contentType` and the UI preview.
pub fn media_type_for(file_name: &str) -> &'static str {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase());
    match extension.as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("txt" | "log" | "csv") => "text/plain",
        Some("json") => "application/fhir+json",
        Some("xml") => "application/fhir+xml",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("dcm") => "application/dicom",
        _ => "application/octet-stream",
    }
}

/// Display name with the extension of the source file, so a downloaded document opens in the
/// right program: an empty name takes the source name, a known wrong extension is replaced,
/// a missing one is appended.
pub fn file_name_with_extension(name: &str, source: &std::path::Path) -> String {
    let name = name.trim();
    let source_name = source
        .file_name()
        .map(|file_name| file_name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some((_, extension)) = source_name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
    else {
        return if name.is_empty() {
            source_name
        } else {
            name.to_owned()
        };
    };
    if name.is_empty() {
        return source_name;
    }
    if name
        .to_lowercase()
        .ends_with(&format!(".{}", extension.to_lowercase()))
    {
        return name.to_owned();
    }
    let stem = match name.rsplit_once('.') {
        Some((stem, _)) if media_type_for(name) != "application/octet-stream" => stem,
        _ => name,
    };
    format!("{stem}.{extension}")
}

/// The issuer's file name is only a hint: no paths, `..` or control characters.
pub fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character == '/' || character == '\\' || character.is_control() {
                '_'
            } else {
                character
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_owned();
    if cleaned.is_empty() {
        "dokument".into()
    } else {
        cleaned
    }
}

/// Patient DHT mode from the battery: below 33% client (also while charging), from 40% or while
/// charging server, in between unchanged; without a readable battery always client.
pub fn patient_dht_server(battery: Option<crate::models::Battery>, current: bool) -> bool {
    use crate::consts::{BATTERY_CLIENT_BELOW_PERCENT, BATTERY_SERVER_FROM_PERCENT};
    match battery {
        None => false,
        Some(battery) if battery.percent < BATTERY_CLIENT_BELOW_PERCENT => false,
        Some(battery) if battery.percent >= BATTERY_SERVER_FROM_PERCENT || battery.charging => true,
        Some(_) => current,
    }
}

/// First battery in `/sys/class/power_supply`; "Full" means plugged in, so it counts as charging.
/// shortcut: Linux and WSL2 only, other systems read no battery (patient stays DHT client).
pub fn read_battery() -> Option<crate::models::Battery> {
    let read = |path: std::path::PathBuf| std::fs::read_to_string(path).ok();
    std::fs::read_dir(crate::consts::POWER_SUPPLY_DIR)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| read(path.join("type")).is_some_and(|kind| kind.trim() == "Battery"))
        .and_then(|path| {
            let percent = read(path.join("capacity"))?.trim().parse().ok()?;
            let status = read(path.join("status")).unwrap_or_default();
            Some(crate::models::Battery {
                percent,
                charging: matches!(status.trim(), "Charging" | "Full"),
            })
        })
}
