//! Détection des logiciels installés hors gestionnaire de paquets :
//! AppImage, exécutables de `~/.local/bin`, applications de `/opt` et
//! entrées de bureau pointant vers un binaire non empaqueté.

use crate::util;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Nature d'un logiciel non empaqueté.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualKind {
    AppImage,
    Desktop,
    OptDir,
    LocalBin,
}

impl ManualKind {
    pub fn label(&self) -> &'static str {
        match self {
            ManualKind::AppImage => "AppImage",
            ManualKind::Desktop => "Entrée .desktop",
            ManualKind::OptDir => "/opt",
            ManualKind::LocalBin => "~/.local/bin",
        }
    }
}

/// Un logiciel installé « à la main ».
#[derive(Debug, Clone)]
pub struct ManualApp {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub kind: ManualKind,
}

/// Analyse un fichier `.desktop` : renvoie (nom affiché, commande).
pub fn parse_desktop_entry(text: &str) -> Option<(String, String)> {
    let mut name = None;
    let mut exec = None;
    let mut in_main_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main_section = line.eq_ignore_ascii_case("[Desktop Entry]");
            continue;
        }
        if !in_main_section {
            continue;
        }
        if let Some(value) = line.strip_prefix("Name=") {
            if name.is_none() && !value.trim().is_empty() {
                name = Some(value.trim().to_string());
            }
        } else if let Some(value) = line.strip_prefix("Exec=") {
            if exec.is_none() {
                exec = Some(clean_exec(value));
            }
        }
    }
    let exec = exec?;
    Some((name.unwrap_or_else(|| exec.clone()), exec))
}

/// Retire guillemets et codes de champ (`%U`, `%f`…) d'une ligne `Exec=`.
pub fn clean_exec(value: &str) -> String {
    let mut cleaned = value.trim().trim_matches('"').to_string();
    if cleaned.starts_with('"') {
        cleaned = cleaned.trim_matches('"').to_string();
    }
    cleaned
        .split_whitespace()
        .filter(|token| !token.starts_with('%') && !token.is_empty())
        .next()
        .unwrap_or("")
        .trim_matches('"')
        .to_string()
}

fn looks_manual(exec_path: &str, home: &Path) -> bool {
    if exec_path.is_empty() {
        return false;
    }
    let path = Path::new(exec_path);
    let in_home = home.to_str().map(|h| exec_path.starts_with(h)).unwrap_or(false);
    in_home
        || exec_path.starts_with("/opt/")
        || exec_path.starts_with("/usr/local/")
        || (!path.is_absolute() && exec_path.contains('/'))
}

/// Détecte les logiciels non empaquetés, en ignorant ceux déjà connus.
pub fn detect(existing: &HashSet<String>) -> Vec<ManualApp> {
    let mut found: Vec<ManualApp> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));

    // 1. AppImage dans les dossiers usuels.
    for folder in [
        "Applications",
        "Bureau",
        "Desktop",
        "Téléchargements",
        "Downloads",
        "bin",
    ] {
        let dir = home.join(folder);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("appimage"))
                .unwrap_or(false)
            {
                push_unique(
                    &mut found,
                    &mut seen_names,
                    &mut seen_paths,
                    &existing,
                    ManualApp {
                        name: entry.file_name().to_string_lossy().to_string(),
                        version: String::new(),
                        path,
                        kind: ManualKind::AppImage,
                    },
                );
            }
        }
    }

    // 2. Exécutables de ~/.local/bin.
    if let Ok(entries) = std::fs::read_dir(home.join(".local/bin")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_file = entry.metadata().map(|meta| meta.is_file()).unwrap_or(false);
            if is_file {
                push_unique(
                    &mut found,
                    &mut seen_names,
                    &mut seen_paths,
                    &existing,
                    ManualApp {
                        name: entry.file_name().to_string_lossy().to_string(),
                        version: String::new(),
                        path,
                        kind: ManualKind::LocalBin,
                    },
                );
            }
        }
    }

    // 3. Exécutables de /usr/local/bin (scripts pip, outils téléchargés).
    if let Ok(entries) = std::fs::read_dir("/usr/local/bin") {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_file = entry.metadata().map(|meta| meta.is_file()).unwrap_or(false);
            if is_file {
                push_unique(
                    &mut found,
                    &mut seen_names,
                    &mut seen_paths,
                    &existing,
                    ManualApp {
                        name: entry.file_name().to_string_lossy().to_string(),
                        version: String::new(),
                        path,
                        kind: ManualKind::LocalBin,
                    },
                );
            }
        }
    }

    // 4. Dossiers de /opt (une application par sous-dossier).
    if let Ok(entries) = std::fs::read_dir("/opt") {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_dir = entry.metadata().map(|meta| meta.is_dir()).unwrap_or(false);
            if is_dir {
                push_unique(
                    &mut found,
                    &mut seen_names,
                    &mut seen_paths,
                    &existing,
                    ManualApp {
                        name: entry.file_name().to_string_lossy().to_string(),
                        version: String::new(),
                        path,
                        kind: ManualKind::OptDir,
                    },
                );
            }
        }
    }

    // 4. Entrées .desktop pointant vers un binaire non empaqueté.
    let mut desktop_dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];
    desktop_dirs.push(home.join(".local/share/applications"));
    for dir in desktop_dirs {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|ext| ext != "desktop").unwrap_or(true) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some((name, exec)) = parse_desktop_entry(&text) else {
                continue;
            };
            if !looks_manual(&exec, &home) {
                continue;
            }
            push_unique(
                &mut found,
                &mut seen_names,
                &mut seen_paths,
                &existing,
                ManualApp {
                    name: desktop_label(&name, &exec),
                    version: String::new(),
                    path,
                    kind: ManualKind::Desktop,
                },
            );
        }
    }

    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found
}

fn push_unique(
    found: &mut Vec<ManualApp>,
    seen_names: &mut HashSet<String>,
    seen_paths: &mut HashSet<PathBuf>,
    existing: &HashSet<String>,
    app: ManualApp,
) {
    let key = app.name.to_lowercase();
    if existing.contains(&key) || seen_names.contains(&key) || seen_paths.contains(&app.path) {
        return;
    }
    seen_names.insert(key);
    seen_paths.insert(app.path.clone());
    found.push(app);
}

/// Nom d'une entrée `.desktop` : le binaire lancé, comme le fait la version
/// Python, pour que le même logiciel porte le même nom dans les deux outils.
fn desktop_label(display: &str, exec: &str) -> String {
    Path::new(exec)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| display.to_string())
}

/// Supprime un logiciel non empaqueté, avec garde-fous de chemin.
pub fn remove(app: &ManualApp) -> Result<String, String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
    match app.kind {
        ManualKind::OptDir => {
            let canonical = app
                .path
                .canonicalize()
                .map_err(|error| format!("chemin introuvable : {error}"))?;
            // Refuse `/opt` lui-même et tout chemin qui en sort.
            if !util::path_within(Path::new("/opt"), &canonical) || canonical == Path::new("/opt") {
                return Err("chemin refusé (hors de /opt)".to_string());
            }
            let target = canonical.to_string_lossy().to_string();
            crate::exec::run_privileged(
                "rm",
                &["-rf".to_string(), "--".to_string(), target],
            )
        }
        ManualKind::Desktop => {
            if !util::path_within(&home, &app.path) && !app.path.starts_with("/usr/local") {
                return Err("entrée de bureau hors des dossiers autorisés".to_string());
            }
            std::fs::remove_file(&app.path)
                .map(|_| format!("entrée supprimée : {}", app.path.display()))
                .map_err(|error| error.to_string())
        }
        _ => {
            let in_home = util::path_within(&home, &app.path);
            if !in_home && !app.path.starts_with("/opt") {
                return Err("chemin hors du dossier personnel".to_string());
            }
            let is_dir = app.path.is_dir();
            let result = if is_dir {
                std::fs::remove_dir_all(&app.path)
            } else {
                std::fs::remove_file(&app.path)
            };
            result
                .map(|_| format!("supprimé : {}", app.path.display()))
                .map_err(|error| error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "[Desktop Entry]\nType=Application\nName=Mon outil\nExec=/home/amine/Applications/tool.AppImage %U\nIcon=tool\n";

    #[test]
    fn parses_desktop_entry() {
        let (name, exec) = parse_desktop_entry(ENTRY).expect("entrée valide");
        assert_eq!(name, "Mon outil");
        assert_eq!(exec, "/home/amine/Applications/tool.AppImage");
    }

    #[test]
    fn ignores_other_sections() {
        let text = "[Desktop Action new]\nName=Nouveau\nExec=/bin/true\n";
        assert!(parse_desktop_entry(text).is_none());
    }

    #[test]
    fn cleans_exec_field_codes() {
        assert_eq!(clean_exec("\"/opt/x/tool\" %F"), "/opt/x/tool");
        assert_eq!(clean_exec("env %U"), "env");
    }

    #[test]
    fn refuses_to_delete_outside_allowed_roots() {
        let app = ManualApp {
            name: "test".to_string(),
            version: String::new(),
            path: PathBuf::from("/etc/passwd"),
            kind: ManualKind::AppImage,
        };
        assert!(remove(&app).is_err());
    }
}
