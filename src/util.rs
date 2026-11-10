//! Petites fonctions partagées : formatage, validation, tailles de dossiers.

use std::path::{Path, PathBuf};

/// Formate un nombre d'octets en unité lisible (base 1024).
pub fn human_bytes(bytes: u64) -> String {
    // Octets et préfixes binaires : « o / Kio » en français, « B / KiB » ailleurs.
    const UNITS: [&str; 6] = ["o", "Kio", "Mio", "Gio", "Tio", "Pio"];
    const UNITS_EN: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let units = if crate::i18n_ui::is_french() { UNITS } else { UNITS_EN };
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index + 1 < UNITS.len() {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{bytes} {}", units[0])
    } else if value >= 100.0 {
        format!("{value:.0} {}", units[index])
    } else if value >= 10.0 {
        format!("{value:.1} {}", units[index])
    } else {
        format!("{value:.2} {}", units[index])
    }
}

/// Lit une taille écrite par un outil (`245,4 MB`, `1.2 GiB`, `512 B`).
pub fn parse_human_size(text: &str) -> u64 {
    let cleaned = text.trim().replace(',', ".");
    let number: String = cleaned
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = number.parse().unwrap_or(0.0);
    let unit: String = cleaned
        .chars()
        .skip(number.len())
        .collect::<String>()
        .trim()
        .to_ascii_uppercase();
    let factor = match unit.as_str() {
        "B" | "" => 1.0,
        "KB" | "K" | "KIB" => 1024.0,
        "MB" | "M" | "MIB" => 1024.0 * 1024.0,
        "GB" | "G" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        "TB" | "T" | "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (value * factor) as u64
}

/// Formate une durée en secondes sous forme `3 j 04 h 12 min`.
pub fn human_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days} j {hours:02} h {minutes:02} min")
    } else if hours > 0 {
        format!("{hours} h {minutes:02} min")
    } else {
        format!("{minutes} min")
    }
}

/// Un nom de paquet valide : minuscules, chiffres, `+`, `-`, `.`, `:` (arch).
pub fn valid_package_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 160 {
        return false;
    }
    if name.starts_with('-') || name.starts_with('.') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | ':' | '_'))
}

/// Une unité systemd valide (`nginx.service`, `user@1000.service`).
pub fn valid_unit_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 200 {
        return false;
    }
    if name.starts_with('-') || name.starts_with('.') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | ':' | '\\'))
}

/// Un identifiant de service Flatpak valide (`org.gnome.Builder`).
pub fn valid_app_id(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 200
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Vérifie qu'un chemin reste sous `root` une fois normalisé, sans lien sortant.
pub fn path_within(root: &Path, candidate: &Path) -> bool {
    let root = normalize(root);
    let candidate = normalize(candidate);
    candidate.starts_with(&root)
}

/// Normalise un chemin sans exiger qu'il existe (gère `.` et `..`).
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Taille d'un dossier, bornée en nombre d'entrées pour rester rapide.
pub fn dir_size(path: &Path, max_entries: usize) -> u64 {
    let mut total = 0;
    let mut seen = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match std::fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > max_entries {
                return total;
            }
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

/// Somme des fichiers correspondant à un motif de suffixe dans un dossier.
pub fn files_with_suffix_size(dir: &Path, suffix: &str) -> u64 {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(suffix)
        })
        .filter_map(|entry| entry.metadata().ok())
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .sum()
}

/// Horodatage local lisible.
pub fn now_human() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Horodatage local au format ISO 8601 (journal, rapports).
pub fn now_iso() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Tronque un texte pour l'affichage tabulaire.
pub fn ellipsis(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Convertit un pourcentage d'occupation en couleur d'alerte (0 = calme, 2 = critique).
pub fn pressure_level(percent: f32) -> u8 {
    if percent >= 90.0 {
        2
    } else if percent >= 75.0 {
        1
    } else {
        0
    }
}

/// Écrit `content` dans un fichier temporaire **privé** : dossier en `0700`
/// sous `$XDG_RUNTIME_DIR` (ou le cache utilisateur), nom unique, création
/// exclusive en `0600`. Sert à préparer un fichier qu'une commande privilégiée
/// copiera ensuite : aucun autre utilisateur ne peut le créer à l'avance, le
/// remplacer ni le lire.
pub fn write_private_temp(hint: &str, content: &str) -> Result<PathBuf, String> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    if hint.is_empty() || !hint.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
        return Err("nom temporaire invalide".to_string());
    }
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(dirs::cache_dir)
        .ok_or("aucun dossier temporaire privé disponible")?;
    let dir = base.join("orbisys");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|error| format!("dossier temporaire : {error}"))?;
    let meta = std::fs::symlink_metadata(&dir).map_err(|error| error.to_string())?;
    let me = std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(u32::MAX);
    if !meta.is_dir() || meta.uid() != me {
        return Err("dossier temporaire inattendu (lien ou autre propriétaire)".to_string());
    }
    if meta.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("{hint}-{}-{nanos}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| format!("écriture temporaire : {error}"))?;
    file.write_all(content.as_bytes()).map_err(|error| error.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_is_readable() {
        assert_eq!(human_bytes(0), "0 o");
        assert_eq!(human_bytes(999), "999 o");
        assert_eq!(human_bytes(2048), "2.00 Kio");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.00 Mio");
    }

    #[test]
    fn parses_sizes_from_tools() {
        assert_eq!(parse_human_size("245,4 MB"), 245 * 1024 * 1024 + 419_430);
        assert_eq!(parse_human_size("1.5 GB"), 1_610_612_736);
        assert_eq!(parse_human_size("512 B"), 512);
        assert_eq!(parse_human_size(""), 0);
    }

    #[test]
    fn package_names_are_validated() {
        assert!(valid_package_name("firefox"));
        assert!(valid_package_name("python3-psutil"));
        assert!(valid_package_name("libc6:amd64"));
        assert!(!valid_package_name(""));
        assert!(!valid_package_name("--force-yes"));
        assert!(!valid_package_name("paquet;rm -rf /"));
        assert!(!valid_package_name("Paquet"));
    }

    #[test]
    fn unit_names_are_validated() {
        assert!(valid_unit_name("nginx.service"));
        assert!(valid_unit_name("user@1000.service"));
        assert!(!valid_unit_name("-reboot"));
        assert!(!valid_unit_name("a;b"));
    }

    #[test]
    fn paths_stay_within_root() {
        assert!(path_within(Path::new("/opt/app"), Path::new("/opt/app/bin/x")));
        assert!(!path_within(Path::new("/opt/app"), Path::new("/opt/app/../etc")));
        assert!(!path_within(Path::new("/opt/app"), Path::new("/etc/passwd")));
    }

    #[test]
    fn private_temp_files_are_exclusive_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let a = write_private_temp("um-test", "contenu").unwrap();
        let b = write_private_temp("um-test", "autre").unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "contenu");
        assert_eq!(std::fs::metadata(&a).unwrap().permissions().mode() & 0o777, 0o600);
        let dir = a.parent().unwrap();
        assert_eq!(std::fs::metadata(dir).unwrap().permissions().mode() & 0o077, 0);
        assert!(write_private_temp("../evil", "x").is_err());
        assert!(write_private_temp("", "x").is_err());
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }
}

/// Reprend les données de l'ancien nom de l'application (`ubuntu-manager`) : dossiers de
/// données, d'état et de configuration renommés en `orbisys` s'ils existent encore et que
/// le nouveau n'existe pas. Un lien symbolique n'est jamais suivi ni déplacé.
pub fn migrate_legacy_dirs() {
    const LEGACY: &str = "ubuntu-manager";
    let home = dirs::home_dir();
    let bases = [
        dirs::data_dir().or_else(|| home.as_ref().map(|h| h.join(".local/share"))),
        dirs::state_dir().or_else(|| home.as_ref().map(|h| h.join(".local/state"))),
        dirs::config_dir().or_else(|| home.as_ref().map(|h| h.join(".config"))),
    ];
    for base in bases.into_iter().flatten() {
        let old = base.join(LEGACY);
        let new = base.join("orbisys");
        let is_real_dir = std::fs::symlink_metadata(&old).map(|m| m.is_dir()).unwrap_or(false);
        if is_real_dir && std::fs::symlink_metadata(&new).is_err() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}
