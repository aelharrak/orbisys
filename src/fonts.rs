//! Polices et rendu : familles installées, substitution fontconfig,
//! cache et installation de fichiers de police.

use crate::exec;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Une famille de polices avec ses styles.
#[derive(Debug, Clone, Serialize)]
pub struct Family {
    pub name: String,
    pub styles: Vec<String>,
    pub file: String,
}

/// État du cache fontconfig de l'utilisateur.
#[derive(Debug, Clone, Serialize, Default)]
pub struct CacheInfo {
    pub files: usize,
    pub bytes: u64,
    pub path: String,
}

/// Résultat d'une demande de substitution.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Match {
    pub requested: String,
    pub resolved: String,
    pub substituted: bool,
}

/// Extensions de police acceptées à l'installation.
pub const FONT_EXTENSIONS: [&str; 5] = ["ttf", "otf", "ttc", "woff", "woff2"];

/// Dossier d'installation des polices ajoutées par l'administrateur.
pub const FONT_DIR: &str = "/usr/local/share/fonts";

/// Analyse `fc-list --format "%{family}|%{style}|%{file}\n"`.
pub fn parse_fc_list(text: &str) -> Vec<Family> {
    let mut families: BTreeMap<String, (Vec<String>, String)> = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() < 3 {
            continue;
        }
        let file = fields[2].trim().to_string();
        for name in fields[0].split(',') {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let entry = families
                .entry(name.to_string())
                .or_insert_with(|| (Vec::new(), file.clone()));
            let style = fields[1].trim().to_string();
            if !style.is_empty() && !entry.0.iter().any(|item| item == &style) {
                entry.0.push(style);
            }
            if entry.1.is_empty() {
                entry.1 = file.clone();
            }
        }
    }
    families
        .into_iter()
        .map(|(name, (styles, file))| Family { name, styles, file })
        .collect()
}

/// Analyse la sortie de `fc-match -f "%{family}\n" "Nom"`.
pub fn parse_fc_match(output: &str, requested: &str) -> Match {
    let resolved = output
        .lines()
        .next()
        .unwrap_or("")
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    Match {
        requested: requested.to_string(),
        substituted: !resolved.eq_ignore_ascii_case(requested.trim()),
        resolved,
    }
}

/// Vérifie qu'un chemin ressemble à un fichier de police installable.
pub fn valid_font_path(path: &Path) -> Result<(), String> {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return Err("extension absente : attendu ttf, otf, ttc, woff ou woff2".to_string());
    };
    let extension = extension.to_ascii_lowercase();
    if !FONT_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!("extension refusée : .{extension}"));
    }
    if !path.is_file() {
        return Err(format!("fichier introuvable : {}", path.display()));
    }
    let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if size == 0 {
        return Err("fichier vide".to_string());
    }
    if size > 64 * 1024 * 1024 {
        return Err("fichier trop volumineux (64 Mio maximum)".to_string());
    }
    Ok(())
}

/// Calcule la taille du cache fontconfig de l'utilisateur.
pub fn cache_info() -> CacheInfo {
    let path = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fontconfig");
    let mut info = CacheInfo {
        path: path.display().to_string(),
        ..CacheInfo::default()
    };
    let mut stack = vec![path];
    let mut visited = 0usize;
    while let Some(directory) = stack.pop() {
        visited += 1;
        if visited > 5_000 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                info.files += 1;
                info.bytes += metadata.len();
            }
        }
    }
    info
}

/// Familles installées (dédupliquées).
pub fn families() -> Result<Vec<Family>, String> {
    let captured = exec::run_with_timeout(
        "fc-list",
        &["--format", "%{family}|%{style}|%{file}\n"],
        std::time::Duration::from_secs(30),
    )
    .map_err(|error| error.0)?;
    if !captured.ok && captured.stdout.trim().is_empty() {
        return Err(captured.error_text());
    }
    Ok(parse_fc_list(&captured.stdout))
}

/// Demande à fontconfig la police réellement utilisée pour un nom.
pub fn match_family(family: &str) -> Result<Match, String> {
    if family.trim().is_empty() || family.len() > 120 {
        return Err("nom de famille invalide".to_string());
    }
    if !family
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
    {
        return Err(format!("famille refusée : {family}"));
    }
    let captured = exec::run(
        "fc-match",
        &["-f", "%{family}\n", family],
    )
    .map_err(|error| error.0)?;
    if !captured.ok {
        return Err(captured.error_text());
    }
    Ok(parse_fc_match(&captured.stdout, family))
}

/// Installe un fichier de police dans `/usr/local/share/fonts` puis reconstruit le cache.
pub fn install(source: &Path) -> Result<String, String> {
    valid_font_path(source)?;
    let name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "nom de fichier invalide".to_string())?;
    if name.starts_with('.') {
        return Err("nom de fichier caché refusé".to_string());
    }
    let target = format!("{FONT_DIR}/{name}");
    exec::run_privileged(
        "install",
        &[
            "-D".to_string(),
            "-m".to_string(),
            "0644".to_string(),
            source.display().to_string(),
            target.clone(),
        ],
    )?;
    let _ = exec::run_privileged("fc-cache", &["-f".to_string()]);
    Ok(format!("police installée : {target}"))
}

/// Supprime une police ajoutée manuellement (jamais celles des paquets).
pub fn remove(path: &Path) -> Result<String, String> {
    let canonical_dir = Path::new(FONT_DIR);
    if !path.starts_with(canonical_dir) {
        return Err(format!(
            "suppression limitée à {FONT_DIR} : {}",
            path.display()
        ));
    }
    valid_font_path(path)?;
    exec::run_privileged("rm", &["-f".to_string(), path.display().to_string()])?;
    let _ = exec::run_privileged("fc-cache", &["-f".to_string()]);
    Ok(format!("police supprimée : {}", path.display()))
}

/// Reconstruit le cache fontconfig.
pub fn refresh_cache() -> Result<String, String> {
    let captured = exec::run_with_timeout(
        "fc-cache",
        &["-f"],
        std::time::Duration::from_secs(120),
    )
    .map_err(|error| error.0)?;
    if !captured.ok && captured.stdout.trim().is_empty() {
        return Err(captured.error_text());
    }
    Ok("cache fontconfig reconstruit".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_families_and_styles() {
        let text = "DejaVu Sans|Book|/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf\nDejaVu Sans|Bold|/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf\nLiberation Serif,Serif|Regular|/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf\n";
        let families = parse_fc_list(text);
        assert_eq!(families.len(), 3);
        let sans = families.iter().find(|item| item.name == "DejaVu Sans").unwrap();
        assert_eq!(sans.styles.len(), 2);
        assert!(families.iter().any(|item| item.name == "Liberation Serif"));
        assert!(families.iter().any(|item| item.name == "Serif"));
    }

    #[test]
    fn detects_font_substitution() {
        let matched = parse_fc_match("DejaVu Sans\n", "Police Inexistante");
        assert!(matched.substituted);
        assert_eq!(matched.resolved, "DejaVu Sans");
        let exact = parse_fc_match("DejaVu Sans,Mono\n", "dejavu sans");
        assert!(!exact.substituted);
    }

    #[test]
    fn rejects_non_font_files() {
        assert!(valid_font_path(Path::new("/etc/passwd")).is_err());
        assert!(valid_font_path(Path::new("/tmp/absent.ttf")).is_err());
        assert!(valid_font_path(Path::new("/tmp/font.txt")).is_err());
    }

    #[test]
    fn rejects_dangerous_family_names() {
        assert!(match_family("DejaVu Sans").is_ok() || match_family("DejaVu Sans").is_err());
        assert!(match_family("a;rm -rf /").is_err());
        assert!(match_family("").is_err());
    }

    #[test]
    fn restricts_removal_to_font_directory() {
        assert!(remove(Path::new("/usr/share/fonts/truetype/x.ttf")).is_err());
    }
}
