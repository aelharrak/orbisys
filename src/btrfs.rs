//! Instantanés Btrfs : montages, sous-volumes, création et suppression.

use crate::exec;
use serde::Serialize;

/// Un montage Btrfs.
#[derive(Debug, Clone, Serialize)]
pub struct BtrfsMount {
    pub target: String,
    pub source: String,
    pub options: String,
}

/// Un sous-volume Btrfs.
#[derive(Debug, Clone, Serialize)]
pub struct Subvolume {
    pub id: u64,
    pub gen: u64,
    pub parent: u64,
    pub path: String,
}

impl Subvolume {
    /// Heuristique : les instantanés sont rangés sous un dossier dédié.
    pub fn is_snapshot(&self) -> bool {
        let lower = self.path.to_ascii_lowercase();
        lower.contains("snapshot") || lower.contains("/snap") || lower.starts_with("@snap")
    }
}

/// Rapport Btrfs.
#[derive(Debug, Clone, Default)]
pub struct BtrfsReport {
    pub tool_available: bool,
    pub mounts: Vec<BtrfsMount>,
    pub subvolumes: Vec<Subvolume>,
    pub errors: Vec<String>,
}

impl BtrfsReport {
    pub fn snapshots(&self) -> usize {
        self.subvolumes
            .iter()
            .filter(|volume| volume.is_snapshot())
            .count()
    }
}

/// Analyse `/proc/mounts` et retient les systèmes de fichiers btrfs.
pub fn parse_mounts(text: &str) -> Vec<BtrfsMount> {
    let mut mounts: Vec<BtrfsMount> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 || fields[2] != "btrfs" {
            continue;
        }
        // Les espaces sont échappés en `\040` dans /proc/mounts.
        let decode = |value: &str| value.replace("\\040", " ");
        let mount = BtrfsMount {
            source: decode(fields[0]),
            target: decode(fields[1]),
            options: decode(fields[3]),
        };
        if !mounts.iter().any(|item| item.target == mount.target) {
            mounts.push(mount);
        }
    }
    mounts
}

/// Analyse `btrfs subvolume list <montage>`.
pub fn parse_subvolumes(text: &str) -> Vec<Subvolume> {
    let mut volumes: Vec<Subvolume> = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 8 {
            continue;
        }
        let number_after = |key: &str| -> Option<u64> {
            let position = tokens.iter().position(|token| *token == key)?;
            tokens.get(position + 1)?.parse().ok()
        };
        let Some(id) = number_after("ID") else {
            continue;
        };
        let gen = number_after("gen").unwrap_or(0);
        let parent = tokens
            .iter()
            .position(|token| *token == "level")
            .and_then(|index| tokens.get(index + 1))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let Some(path_index) = tokens.iter().position(|token| *token == "path") else {
            continue;
        };
        let path = tokens[path_index + 1..].join(" ");
        if path.is_empty() {
            continue;
        }
        if volumes.iter().any(|volume| volume.id == id) {
            continue;
        }
        volumes.push(Subvolume {
            id,
            gen,
            parent,
            path,
        });
    }
    volumes.sort_by(|a, b| a.path.cmp(&b.path));
    volumes
}

/// Chemin absolu utilisable avec `btrfs`.
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && path.starts_with('/')
        && !path.contains("..")
        && !path.ends_with('/')
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '@'))
        || path == "/"
}

/// Crée un instantané en lecture seule.
pub fn create_snapshot(source: &str, destination: &str) -> Result<String, String> {
    if !valid_path(source) || !valid_path(destination) {
        return Err("chemins refusés : attendu des chemins absolus sans `..`".to_string());
    }
    if source == destination {
        return Err("la source et la destination sont identiques".to_string());
    }
    if std::path::Path::new(destination).exists() {
        return Err(format!("la destination existe déjà : {destination}"));
    }
    if let Some(parent) = std::path::Path::new(destination).parent() {
        if !parent.exists() {
            return Err(format!("dossier parent absent : {}", parent.display()));
        }
    }
    exec::run_privileged(
        "btrfs",
        &[
            "subvolume".to_string(),
            "snapshot".to_string(),
            "-r".to_string(),
            source.to_string(),
            destination.to_string(),
        ],
    )?;
    Ok(format!("instantané créé : {destination}"))
}

/// Supprime un sous-volume (instantané).
pub fn delete_subvolume(path: &str) -> Result<String, String> {
    if !valid_path(path) || path == "/" {
        return Err(format!("chemin refusé : {path}"));
    }
    exec::run_privileged(
        "btrfs",
        &[
            "subvolume".to_string(),
            "delete".to_string(),
            path.to_string(),
        ],
    )?;
    Ok(format!("sous-volume supprimé : {path}"))
}

/// Collecte les montages et sous-volumes btrfs.
pub fn report() -> BtrfsReport {
    let mut report = BtrfsReport {
        tool_available: exec::exists("btrfs"),
        ..BtrfsReport::default()
    };
    if let Ok(text) = std::fs::read_to_string("/proc/mounts") {
        report.mounts = parse_mounts(&text);
    }
    if !report.tool_available {
        report
            .errors
            .push("btrfs-progs absent : sous-volumes illisibles".to_string());
        return report;
    }
    for mount in &report.mounts {
        match exec::run_with_timeout(
            "btrfs",
            &["subvolume", "list", &mount.target],
            std::time::Duration::from_secs(30),
        ) {
            Ok(captured) => {
                if captured.ok {
                    report.subvolumes.extend(parse_subvolumes(&captured.stdout));
                } else if !captured.stdout.trim().is_empty() {
                    report.subvolumes.extend(parse_subvolumes(&captured.stdout));
                } else {
                    report
                        .errors
                        .push(format!("{} : {}", mount.target, captured.error_text()));
                }
            }
            Err(error) => report.errors.push(format!("{} : {}", mount.target, error.0)),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_btrfs_mounts() {
        let text = "/dev/sda2 / btrfs rw,relatime,subvolid=256 0 0\n/dev/sdb1 /home\\040dir ext4 rw 0 0\n/dev/sda2 /home btrfs rw,subvol=@home 0 0\n";
        let mounts = parse_mounts(text);
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].target, "/");
        assert_eq!(mounts[1].target, "/home");
    }

    #[test]
    fn decodes_escaped_mount_points() {
        let mounts = parse_mounts("/dev/sda2 /mnt/mon\\040disque btrfs rw 0 0\n");
        assert_eq!(mounts[0].target, "/mnt/mon disque");
    }

    #[test]
    fn parses_subvolume_listing() {
        let text = "ID 256 gen 1234 top level 5 path @\nID 257 gen 1230 top level 5 path @home\nID 300 gen 42 top level 257 path @/snapshots/home-2026-11-02\n";
        let volumes = parse_subvolumes(text);
        assert_eq!(volumes.len(), 3);
        assert_eq!(volumes[0].id, 256);
        assert_eq!(volumes[0].parent, 5);
        assert_eq!(volumes[0].path, "@");
        let snapshot = volumes.iter().find(|volume| volume.id == 300).unwrap();
        assert!(snapshot.is_snapshot());
        assert!(!volumes.iter().find(|volume| volume.id == 257).unwrap().is_snapshot());
    }

    #[test]
    fn flags_snapshot_paths() {
        for path in ["@snapshots/root", "@snap-home", "snapshots/x"] {
            let volume = Subvolume {
                id: 1,
                gen: 1,
                parent: 5,
                path: path.to_string(),
            };
            assert!(volume.is_snapshot(), "{path}");
        }
        let plain = Subvolume {
            id: 2,
            gen: 1,
            parent: 5,
            path: "@data".to_string(),
        };
        assert!(!plain.is_snapshot());
    }

    #[test]
    fn rejects_unsafe_paths() {
        assert!(valid_path("/home/snapshots/a"));
        assert!(valid_path("/"));
        assert!(!valid_path("home/snapshots/a"));
        assert!(!valid_path("/home/../etc"));
        assert!(!valid_path("/home/snap; reboot"));
        assert!(!valid_path(""));
        assert!(delete_subvolume("/").is_err());
        assert!(delete_subvolume("relative").is_err());
    }

    #[test]
    fn refuses_identical_or_existing_destination() {
        assert!(create_snapshot("/home", "/home").is_err());
        assert!(create_snapshot("home", "/mnt/x").is_err());
    }
}
