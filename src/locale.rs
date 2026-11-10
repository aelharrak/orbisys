//! Polices et locales : langue du système, clavier, fuseau horaire, NTP et
//! familles de polices installées.

use crate::exec;
use serde::Serialize;
use std::collections::BTreeMap;

/// Configuration de langue et d'heure du système.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LocaleInfo {
    pub session_lang: String,
    pub system_lang: String,
    pub keymap: String,
    pub timezone: String,
    pub synchronized: bool,
    pub ntp: bool,
    pub available_locales: usize,
}

/// Une famille de polices et son nombre de styles.
#[derive(Debug, Clone, Serialize)]
pub struct FontFamily {
    pub family: String,
    pub styles: usize,
}

/// État de la page Polices & locales.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LocaleReport {
    pub locale: LocaleInfo,
    pub fonts: Vec<FontFamily>,
    pub default_sans: String,
    pub default_mono: String,
    pub font_total: usize,
}

/// Un code de locale valide (`fr_FR.UTF-8`, `en_US.utf8`, `C`).
pub fn valid_locale(code: &str) -> bool {
    if code.is_empty() || code.len() > 40 {
        return false;
    }
    if code == "C" || code == "POSIX" || code == "C.UTF-8" {
        return true;
    }
    code.contains('_')
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '@'))
}

/// Un fuseau horaire valide (`Europe/Paris`, `America/Argentina/Buenos_Aires`).
pub fn valid_timezone(zone: &str) -> bool {
    if zone.is_empty() || zone.len() > 60 || !zone.contains('/') {
        return false;
    }
    if zone.contains("..") || zone.starts_with('/') {
        return false;
    }
    zone.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+'))
}

/// Analyse la sortie de `locale` (`LANG=…`, `LC_*=…`).
pub fn parse_locale_env(text: &str) -> (String, Vec<(String, String)>) {
    let mut lang = String::new();
    let mut entries = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        if key == "LANG" {
            lang = value.clone();
        }
        if !value.is_empty() {
            entries.push((key.to_string(), value));
        }
    }
    (lang, entries)
}

/// Analyse `localectl status`.
pub fn parse_localectl(text: &str) -> (String, String, String) {
    let mut locale = String::new();
    let mut keymap = String::new();
    let mut layout = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("System Locale:") {
            for part in rest.split_whitespace() {
                if let Some(value) = part.strip_prefix("LANG=") {
                    locale = value.to_string();
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("VC Keymap:") {
            keymap = rest.trim().to_string();
        } else if let Some(rest) = trimmed.strip_prefix("X11 Layout:") {
            layout = rest.trim().to_string();
        }
    }
    if keymap.is_empty() {
        keymap = layout.clone();
    }
    (locale, keymap, layout)
}

/// Analyse `timedatectl` (fuseau, synchronisation, NTP).
pub fn parse_timedatectl(text: &str) -> (String, bool, bool) {
    let mut zone = String::new();
    let mut synchronized = false;
    let mut ntp = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Time zone:") {
            zone = rest.split_whitespace().next().unwrap_or("").to_string();
        } else if let Some(rest) = trimmed.strip_prefix("System clock synchronized:") {
            synchronized = rest.trim().eq_ignore_ascii_case("yes");
        } else if let Some(rest) = trimmed.strip_prefix("NTP service:") {
            ntp = rest.trim().eq_ignore_ascii_case("active");
        }
    }
    (zone, synchronized, ntp)
}

/// Analyse `fc-list : family style` (`DejaVu Sans:style=Bold`).
pub fn parse_fc_list(text: &str) -> Vec<FontFamily> {
    let mut families: BTreeMap<String, usize> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (family_part, style_part) = match line.split_once(":style=") {
            Some((family, style)) => (family, style),
            None => (line, ""),
        };
        // Une ligne peut lister plusieurs familles séparées par des virgules.
        for family in family_part.split(',') {
            let family = family.trim();
            if family.is_empty() {
                continue;
            }
            let entry = families.entry(family.to_string()).or_insert(0);
            if !style_part.is_empty() {
                *entry += 1;
            } else {
                *entry = (*entry).max(1);
            }
        }
    }
    let mut fonts: Vec<FontFamily> = families
        .into_iter()
        .map(|(family, styles)| FontFamily { family, styles })
        .collect();
    fonts.sort_by(|a, b| a.family.cmp(&b.family));
    fonts
}

fn locale_count() -> usize {
    std::fs::read_dir("/usr/share/i18n/locales")
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_file())
                .count()
        })
        .unwrap_or(0)
}

/// Lit l'état des locales et des polices.
pub fn report() -> LocaleReport {
    let (session_lang, _) = exec::run("locale", &[])
        .map(|captured| parse_locale_env(&captured.stdout))
        .unwrap_or_default();

    let (system_lang, keymap, _) = exec::run("localectl", &["status"])
        .map(|captured| parse_localectl(&captured.stdout))
        .unwrap_or_default();

    let (timezone, synchronized, ntp) = exec::run("timedatectl", &[])
        .map(|captured| parse_timedatectl(&captured.stdout))
        .unwrap_or_default();

    let fonts = exec::run("fc-list", &[":", "family", "style"])
        .map(|captured| parse_fc_list(&captured.stdout))
        .unwrap_or_default();

    let default_sans = exec::run("fc-match", &["-f", "%{family}", "sans-serif"])
        .map(|captured| captured.text())
        .unwrap_or_default();
    let default_mono = exec::run("fc-match", &["-f", "%{family}", "monospace"])
        .map(|captured| captured.text())
        .unwrap_or_default();

    LocaleReport {
        locale: LocaleInfo {
            session_lang,
            system_lang,
            keymap,
            timezone,
            synchronized,
            ntp,
            available_locales: locale_count(),
        },
        font_total: fonts.len(),
        fonts,
        default_sans,
        default_mono,
    }
}

/// Définit la locale du système (Polkit).
pub fn set_system_locale(code: &str) -> Result<String, String> {
    if !valid_locale(code) {
        return Err("locale invalide : format attendu fr_FR.UTF-8".to_string());
    }
    if !exec::exists("localectl") {
        return Err("localectl n'est pas disponible".to_string());
    }
    exec::run_privileged(
        "localectl",
        &["set-locale".to_string(), format!("LANG={code}")],
    )?;
    Ok(format!("locale système : {code}"))
}

/// Définit le fuseau horaire (Polkit).
pub fn set_timezone(zone: &str) -> Result<String, String> {
    if !valid_timezone(zone) {
        return Err("fuseau invalide : format attendu Europe/Paris".to_string());
    }
    if !exec::exists("timedatectl") {
        return Err("timedatectl n'est pas disponible".to_string());
    }
    exec::run_privileged("timedatectl", &["set-timezone".to_string(), zone.to_string()])?;
    Ok(format!("fuseau horaire : {zone}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_are_validated() {
        assert!(valid_locale("fr_FR.UTF-8"));
        assert!(valid_locale("en_US.utf8"));
        assert!(valid_locale("C.UTF-8"));
        assert!(!valid_locale(""));
        assert!(!valid_locale("fr"));
        assert!(!valid_locale("fr_FR;reboot"));
    }

    #[test]
    fn timezones_are_validated() {
        assert!(valid_timezone("Europe/Paris"));
        assert!(valid_timezone("America/Argentina/Buenos_Aires"));
        assert!(!valid_timezone("Paris"));
        assert!(!valid_timezone("../etc"));
        assert!(!valid_timezone("/etc/passwd"));
        assert!(!valid_timezone("Europe/Paris;reboot"));
    }

    #[test]
    fn parses_locale_environment() {
        let text = "LANG=fr_FR.UTF-8\nLC_CTYPE=\"fr_FR.UTF-8\"\nLC_ALL=\n";
        let (lang, entries) = parse_locale_env(text);
        assert_eq!(lang, "fr_FR.UTF-8");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].1, "fr_FR.UTF-8");
    }

    #[test]
    fn parses_localectl_status() {
        let text = "   System Locale: LANG=fr_FR.UTF-8\n       VC Keymap: fr\n      X11 Layout: fr\n";
        let (locale, keymap, layout) = parse_localectl(text);
        assert_eq!(locale, "fr_FR.UTF-8");
        assert_eq!(keymap, "fr");
        assert_eq!(layout, "fr");
    }

    #[test]
    fn parses_timedatectl() {
        let text = "               Time zone: Europe/Paris (CET, +0100)\nSystem clock synchronized: yes\n              NTP service: active\n";
        let (zone, synchronized, ntp) = parse_timedatectl(text);
        assert_eq!(zone, "Europe/Paris");
        assert!(synchronized);
        assert!(ntp);
    }

    #[test]
    fn parses_font_families_and_styles() {
        let text = "DejaVu Sans:style=Bold\nDejaVu Sans:style=Book\nLiberation Mono:style=Regular\nNoto Sans,Noto Sans CJK:style=Regular\n";
        let fonts = parse_fc_list(text);
        let dejavu = fonts.iter().find(|font| font.family == "DejaVu Sans").unwrap();
        assert_eq!(dejavu.styles, 2);
        assert_eq!(fonts.len(), 4);
        assert!(fonts.iter().any(|font| font.family == "Noto Sans CJK"));
    }
}
