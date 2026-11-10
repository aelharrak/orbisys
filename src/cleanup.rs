//! Nettoyage : cibles proposées, taille mesurée, action exécutée après
//! confirmation. Les cibles système demandent Polkit, les cibles
//! personnelles restent dans le dossier de l'utilisateur.

use crate::distro::Capabilities;
use crate::exec;
use crate::util;
use std::path::{Path, PathBuf};

/// Type de nettoyage (détermine l'action exécutée).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupKind {
    AptCache,
    Journal,
    Trash,
    UserCache,
    Thumbnails,
    OldKernels,
    SnapOldRevisions,
}

/// Une cible de nettoyage avec sa taille actuelle.
#[derive(Debug, Clone)]
pub struct CleanupItem {
    pub id: &'static str,
    pub label: String,
    pub description: String,
    pub size: u64,
    pub requires_root: bool,
    pub kind: CleanupKind,
}

impl CleanupItem {
    pub fn removable(&self) -> bool {
        self.size > 0
    }
}

fn journal_usage() -> u64 {
    let Ok(captured) = exec::run("journalctl", &["--disk-usage"]) else {
        return 0;
    };
    captured
        .stdout
        .split_whitespace()
        .find(|token| token.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
        .map(util::parse_human_size)
        .unwrap_or(0)
}

fn old_kernels() -> Vec<String> {
    let running = crate::distro::kernel_release();
    let Ok(entries) = std::fs::read_dir("/boot") else {
        return Vec::new();
    };
    let mut kernels: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let version = name.strip_prefix("vmlinuz-")?.to_string();
            Some(version)
        })
        .filter(|version| !version.contains(&running) && !running.is_empty())
        .collect();
    kernels.sort();
    kernels.dedup();
    kernels
}

/// Révisions Snap désactivées (anciennes versions conservées).
pub fn snap_disabled_revisions() -> Vec<(String, String)> {
    let Ok(captured) = exec::run("snap", &["list", "--all"]) else {
        return Vec::new();
    };
    let mut revisions = Vec::new();
    for line in captured.stdout.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        let notes = fields[5.min(fields.len() - 1)];
        if line.contains("disabled") {
            revisions.push((fields[0].to_string(), fields[2].to_string()));
        } else if notes.contains("disabled") {
            revisions.push((fields[0].to_string(), fields[2].to_string()));
        }
    }
    revisions
}

fn trash_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".local/share/Trash"))
}

/// Liste les cibles de nettoyage et mesure leur intérêt.
pub fn scan(capabilities: &Capabilities) -> Vec<CleanupItem> {
    let mut items = Vec::new();

    if capabilities.ufw || exec::exists("dpkg-query") {
        items.push(CleanupItem {
            id: "apt-cache",
            label: "Cache des paquets téléchargés".to_string(),
            description: "Archives .deb déjà installées, dans /var/cache/apt/archives."
                .to_string(),
            size: util::files_with_suffix_size(Path::new("/var/cache/apt/archives"), ".deb"),
            requires_root: true,
            kind: CleanupKind::AptCache,
        });
    }

    if capabilities.journalctl {
        items.push(CleanupItem {
            id: "journal",
            label: "Journal systemd".to_string(),
            description: "Anciennes entrées du journal, réduites à 200 Mio.".to_string(),
            size: journal_usage(),
            requires_root: true,
            kind: CleanupKind::Journal,
        });
    }

    if let Some(trash) = trash_dir() {
        items.push(CleanupItem {
            id: "trash",
            label: "Corbeille".to_string(),
            description: "Fichiers placés dans la corbeille.".to_string(),
            size: util::dir_size(&trash.join("files"), 100_000),
            requires_root: false,
            kind: CleanupKind::Trash,
        });
    }

    if let Some(cache) = dirs::cache_dir() {
        items.push(CleanupItem {
            id: "user-cache",
            label: "Cache utilisateur".to_string(),
            description: "Caches applicatifs de ~/.cache (vignettes incluses).".to_string(),
            size: util::dir_size(&cache, 300_000),
            requires_root: false,
            kind: CleanupKind::UserCache,
        });
        items.push(CleanupItem {
            id: "thumbnails",
            label: "Vignettes".to_string(),
            description: "Aperçus d'images et de vidéos régénérés automatiquement.".to_string(),
            size: util::dir_size(&cache.join("thumbnails"), 100_000),
            requires_root: false,
            kind: CleanupKind::Thumbnails,
        });
    }

    let kernels = old_kernels();
    if !kernels.is_empty() {
        items.push(CleanupItem {
            id: "old-kernels",
            label: "Noyaux obsolètes".to_string(),
            description: format!("{} noyau(x) plus ancien(s) que celui en cours : {}.", kernels.len(), kernels.join(", ")),
            size: 0,
            requires_root: true,
            kind: CleanupKind::OldKernels,
        });
    }

    let revisions = snap_disabled_revisions();
    if !revisions.is_empty() {
        items.push(CleanupItem {
            id: "snap-revisions",
            label: "Anciennes révisions Snap".to_string(),
            description: format!("{} révision(s) désactivée(s) conservées par snap.", revisions.len()),
            size: 0,
            requires_root: true,
            kind: CleanupKind::SnapOldRevisions,
        });
    }

    items
}

fn remove_children(dir: &Path) -> Result<usize, String> {
    let entries = std::fs::read_dir(dir).map_err(|error| error.to_string())?;
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if result.is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Exécute une cible de nettoyage après confirmation.
pub fn run(item: &CleanupItem) -> Result<String, String> {
    match item.kind {
        CleanupKind::AptCache => exec::run_privileged("apt-get", &["clean".to_string()]),
        CleanupKind::Journal => exec::run_privileged(
            "journalctl",
            &["--vacuum-size=200M".to_string()],
        ),
        CleanupKind::OldKernels => exec::run_privileged(
            "apt-get",
            &[
                "autoremove".to_string(),
                "--purge".to_string(),
                "-y".to_string(),
            ],
        ),
        CleanupKind::Trash => {
            let trash = trash_dir().ok_or("dossier personnel introuvable")?;
            let files = remove_children(&trash.join("files"))?;
            let _ = remove_children(&trash.join("info"));
            Ok(format!("{files} élément(s) retiré(s) de la corbeille"))
        }
        CleanupKind::UserCache => {
            let cache = dirs::cache_dir().ok_or("dossier de cache introuvable")?;
            let removed = remove_children(&cache)?;
            Ok(format!("{removed} entrée(s) de cache supprimée(s)"))
        }
        CleanupKind::Thumbnails => {
            let cache = dirs::cache_dir().ok_or("dossier de cache introuvable")?;
            let removed = remove_children(&cache.join("thumbnails"))?;
            Ok(format!("{removed} vignette(s) supprimée(s)"))
        }
        CleanupKind::SnapOldRevisions => {
            let revisions = snap_disabled_revisions();
            if revisions.is_empty() {
                return Ok("aucune révision désactivée".to_string());
            }
            let mut messages = Vec::new();
            for (name, revision) in revisions {
                if !util::valid_package_name(&name) {
                    continue;
                }
                let result = exec::run_privileged(
                    "snap",
                    &[
                        "remove".to_string(),
                        name.clone(),
                        "--revision".to_string(),
                        revision.clone(),
                    ],
                );
                match result {
                    Ok(_) => messages.push(format!("{name} r{revision} supprimée")),
                    Err(error) => messages.push(format!("{name} : {error}")),
                }
            }
            Ok(messages.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_items_are_not_removable() {
        let item = CleanupItem {
            id: "trash",
            label: "Corbeille".to_string(),
            description: String::new(),
            size: 0,
            requires_root: false,
            kind: CleanupKind::Trash,
        };
        assert!(!item.removable());
    }

    #[test]
    fn removing_children_empties_directory() {
        let base = std::env::temp_dir().join(format!("um-cleanup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("sub")).unwrap();
        std::fs::write(base.join("fichier.txt"), b"x").unwrap();
        std::fs::write(base.join("sub/autre.txt"), b"y").unwrap();
        assert_eq!(remove_children(&base).unwrap(), 2);
        assert_eq!(std::fs::read_dir(&base).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&base);
    }
}
