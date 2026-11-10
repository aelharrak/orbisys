//! Bureau GNOME : apparence via gsettings, extensions, raccourcis, polices.

use crate::exec;
use crate::util;
use serde::Serialize;

/// Réglages d'apparence lus via gsettings.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Appearance {
    pub gtk_theme: String,
    pub color_scheme: String,
    pub icon_theme: String,
    pub font: String,
    pub background: String,
    pub button_layout: String,
    pub themes: Vec<String>,
    pub icons: Vec<String>,
}

/// Une extension GNOME Shell.
#[derive(Debug, Clone, Serialize)]
pub struct Extension {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

/// Un raccourci clavier de GNOME.
#[derive(Debug, Clone, Serialize)]
pub struct Shortcut {
    pub schema: String,
    pub key: String,
    pub value: String,
}

/// État de la page Bureau GNOME.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DesktopReport {
    pub appearance: Appearance,
    pub extensions: Vec<Extension>,
    pub shortcuts: Vec<Shortcut>,
    pub fonts: Vec<String>,
    pub available: bool,
}

/// Une clé gsettings valide (`gtk-theme`).
pub fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && !key.starts_with('-')
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// Un identifiant d'extension valide (`dash-to-dock@micxgx.gmail.com`).
pub fn valid_extension_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'))
}

/// Analyse `gsettings list-recursively schéma`.
pub fn parse_gsettings_list(text: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.splitn(3, ' ').collect();
        if fields.len() < 3 {
            continue;
        }
        let key = fields[1].trim();
        if !valid_key(key) {
            continue;
        }
        entries.push((key.to_string(), fields[2].trim().trim_matches('\'').to_string()));
    }
    entries
}

/// Analyse `gnome-extensions list` et sa variante `--enabled`.
pub fn parse_extensions(all: &str, enabled: &str) -> Vec<Extension> {
    let enabled_ids: Vec<&str> = enabled.lines().map(str::trim).collect();
    let mut extensions = Vec::new();
    for line in all.lines() {
        let id = line.trim();
        if !valid_extension_id(id) {
            continue;
        }
        let name = id.split('@').next().unwrap_or(id).replace('-', " ");
        extensions.push(Extension {
            id: id.to_string(),
            name,
            enabled: enabled_ids.contains(&id),
        });
    }
    extensions.sort_by(|a, b| a.id.cmp(&b.id));
    extensions
}

fn read_setting(schema: &str, key: &str) -> String {
    exec::run("gsettings", &["get", schema, key])
        .map(|captured| captured.text().trim_matches('\'').to_string())
        .unwrap_or_default()
}

fn directory_names(path: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Lit l'état du bureau.
pub fn report() -> DesktopReport {
    let available = exec::exists("gsettings");
    if !available {
        return DesktopReport::default();
    }
    let appearance = Appearance {
        gtk_theme: read_setting("org.gnome.desktop.interface", "gtk-theme"),
        color_scheme: read_setting("org.gnome.desktop.interface", "color-scheme"),
        icon_theme: read_setting("org.gnome.desktop.interface", "icon-theme"),
        font: read_setting("org.gnome.desktop.interface", "font-name"),
        background: read_setting("org.gnome.desktop.background", "picture-uri"),
        button_layout: read_setting(
            "org.gnome.desktop.wm.preferences",
            "button-layout",
        ),
        themes: directory_names("/usr/share/themes"),
        icons: directory_names("/usr/share/icons"),
    };

    let extensions = if exec::exists("gnome-extensions") {
        let all = exec::run("gnome-extensions", &["list"])
            .map(|captured| captured.stdout)
            .unwrap_or_default();
        let enabled = exec::run("gnome-extensions", &["list", "--enabled"])
            .map(|captured| captured.stdout)
            .unwrap_or_default();
        parse_extensions(&all, &enabled)
    } else {
        Vec::new()
    };

    let shortcuts = exec::run(
        "gsettings",
        &[
            "list-recursively",
            "org.gnome.desktop.wm.keybindings",
        ],
    )
    .map(|captured| {
        parse_gsettings_list(&captured.stdout)
            .into_iter()
            .filter(|(_, value)| !value.is_empty() && value != "@as []")
            .map(|(key, value)| Shortcut {
                schema: "org.gnome.desktop.wm.keybindings".to_string(),
                key,
                value,
            })
            .collect()
    })
    .unwrap_or_default();

    let fonts = exec::run("fc-list", &[":", "family"])
        .map(|captured| {
            let mut families: Vec<String> = captured
                .stdout
                .lines()
                .flat_map(|line| line.split(','))
                .map(|family| family.trim().to_string())
                .filter(|family| !family.is_empty())
                .collect();
            families.sort();
            families.dedup();
            families.truncate(40);
            families
        })
        .unwrap_or_default();

    DesktopReport {
        appearance,
        extensions,
        shortcuts,
        fonts,
        available,
    }
}

/// Écrit une valeur gsettings (niveau utilisateur, sans privilège).
pub fn set_setting(schema: &str, key: &str, value: &str) -> Result<String, String> {
    if !valid_key(key) || schema.is_empty() || !schema.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) {
        return Err("schéma ou clé invalide".to_string());
    }
    if value.contains('\n') || value.contains('\0') {
        return Err("valeur refusée".to_string());
    }
    let captured = exec::run("gsettings", &["set", schema, key, value])
        .map_err(|error| error.0)?;
    if captured.ok {
        Ok(format!("{key} = {value}"))
    } else {
        Err(captured.error_text())
    }
}

/// Active ou désactive une extension.
pub fn set_extension(id: &str, enabled: bool) -> Result<String, String> {
    if !valid_extension_id(id) {
        return Err("identifiant d'extension invalide".to_string());
    }
    if !exec::exists("gnome-extensions") {
        return Err("gnome-extensions n'est pas disponible".to_string());
    }
    let action = if enabled { "enable" } else { "disable" };
    let captured = exec::run("gnome-extensions", &[action, id]).map_err(|error| error.0)?;
    if captured.ok {
        Ok(format!(
            "extension {id} {}",
            if enabled { "activée" } else { "désactivée" }
        ))
    } else {
        Err(captured.error_text())
    }
}

/// Liste les thèmes correspondant à une recherche.
pub fn matching<'a>(items: &'a [String], query: &str) -> Vec<&'a String> {
    let query = query.to_lowercase();
    items
        .iter()
        .filter(|item| query.is_empty() || item.to_lowercase().contains(&query))
        .collect()
}

/// Résumé lisible d'un raccourci (première combinaison).
pub fn shortcut_label(value: &str) -> String {
    let cleaned = value
        .trim_matches(|c| c == '[' || c == ']')
        .replace('\'', "");
    util::ellipsis(&cleaned, 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_validated() {
        assert!(valid_key("gtk-theme"));
        assert!(valid_key("color-scheme"));
        assert!(!valid_key(""));
        assert!(!valid_key("-x"));
        assert!(!valid_key("a b"));
    }

    #[test]
    fn extension_ids_are_validated() {
        assert!(valid_extension_id("dash-to-dock@micxgx.gmail.com"));
        assert!(valid_extension_id("user-theme@gnome-shell-extensions.gcampax.github.com"));
        assert!(!valid_extension_id(""));
        assert!(!valid_extension_id("-x"));
        assert!(!valid_extension_id("a;reboot"));
    }

    #[test]
    fn parses_gsettings_output() {
        let text = "org.gnome.desktop.interface gtk-theme 'Yaru-dark'\norg.gnome.desktop.interface font-name 'Cantarell 11'\n";
        let entries = parse_gsettings_list(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "gtk-theme");
        assert_eq!(entries[0].1, "Yaru-dark");
        assert_eq!(entries[1].1, "Cantarell 11");
    }

    #[test]
    fn parses_extensions_and_states() {
        let all = "dash-to-dock@micxgx.gmail.com\nuser-theme@gnome-shell-extensions.gcampax.github.com\n";
        let enabled = "user-theme@gnome-shell-extensions.gcampax.github.com\n";
        let extensions = parse_extensions(all, enabled);
        assert_eq!(extensions.len(), 2);
        let dock = extensions
            .iter()
            .find(|entry| entry.id.starts_with("dash-to-dock"))
            .unwrap();
        assert!(!dock.enabled);
        assert_eq!(dock.name, "dash to dock");
        let theme = extensions
            .iter()
            .find(|entry| entry.id.starts_with("user-theme"))
            .unwrap();
        assert!(theme.enabled);
    }

    #[test]
    fn shortcuts_are_cleaned() {
        assert_eq!(shortcut_label("['<Alt>F4']"), "<Alt>F4");
        assert_eq!(shortcut_label("['<Super>e', '<Super>E']"), "<Super>e, <Super>E");
    }

    #[test]
    fn filters_themes() {
        let items = vec!["Adwaita".to_string(), "Yaru-dark".to_string(), "Yaru".to_string()];
        assert_eq!(matching(&items, "yaru").len(), 2);
        assert_eq!(matching(&items, "").len(), 3);
        assert_eq!(matching(&items, "gnome").len(), 0);
    }

    #[test]
    fn rejects_bad_settings() {
        assert!(set_setting("org.gnome.desktop.interface", "gtk-theme; rm -rf /", "x").is_err());
        assert!(set_setting("", "gtk-theme", "x").is_err());
        assert!(set_setting("org.gnome.desktop.interface", "gtk-theme", "a\nb").is_err());
        assert!(set_extension("a;reboot", true).is_err());
    }
}
