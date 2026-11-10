//! Stockage : appareils (`lsblk`), montages (`findmnt`), `/etc/fstab`,
//! Btrfs et instantanés Timeshift, plus la recherche de doublons.

use crate::exec;
use crate::util;
use serde::Serialize;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};

/// Un appareil ou une partition décrit par `lsblk`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BlockDevice {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub fstype: String,
    pub mountpoint: String,
    pub model: String,
    pub label: String,
    pub children: Vec<BlockDevice>,
}

impl BlockDevice {
    /// Parcours à plat, avec un niveau d'indentation pour l'affichage.
    pub fn flatten(&self, depth: usize, out: &mut Vec<(usize, BlockDevice)>) {
        out.push((depth, self.clone()));
        for child in &self.children {
            child.flatten(depth + 1, out);
        }
    }
}

/// Une entrée de `/etc/fstab`.
#[derive(Debug, Clone, Serialize)]
pub struct FstabEntry {
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub options: String,
}

/// Un montage actif (`findmnt`).
#[derive(Debug, Clone, Serialize)]
pub struct MountInfo {
    pub target: String,
    pub source: String,
    pub fstype: String,
    pub options: String,
}

/// Un instantané Timeshift.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub name: String,
    pub tags: String,
    pub created: String,
}

/// Regroupement de fichiers identiques.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub size: u64,
    pub fingerprint: String,
    pub paths: Vec<PathBuf>,
}

/// Résultat d'une recherche de doublons.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DuplicateScan {
    pub groups: Vec<DuplicateGroup>,
    pub files_scanned: usize,
    pub bytes_duplicated: u64,
    pub truncated: bool,
    pub root: String,
}

/// Instantané complet de l'état du stockage.
#[derive(Debug, Clone, Default)]
pub struct StorageReport {
    pub devices: Vec<(usize, BlockDevice)>,
    pub mounts: Vec<MountInfo>,
    pub fstab: Vec<FstabEntry>,
    pub btrfs: Vec<String>,
    pub snapshots: Vec<Snapshot>,
    pub errors: Vec<String>,
}

/// Analyse la sortie JSON de `lsblk`.
pub fn parse_lsblk(text: &str) -> Vec<BlockDevice> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(devices) = value.get("blockdevices").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    devices.iter().filter_map(parse_device).collect()
}

fn text_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|field| field.as_str())
        .unwrap_or("")
        .to_string()
}

fn parse_device(value: &serde_json::Value) -> Option<BlockDevice> {
    let name = value.get("name").and_then(|field| field.as_str())?;
    let device = BlockDevice {
        name: name.to_string(),
        path: value
            .get("path")
            .and_then(|field| field.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("/dev/{name}")),
        kind: text_field(value, "type"),
        size: value
            .get("size")
            .and_then(|field| field.as_u64())
            .or_else(|| {
                value
                    .get("size")
                    .and_then(|field| field.as_str())
                    .and_then(|size| size.parse::<u64>().ok())
            })
            .unwrap_or(0),
        fstype: text_field(value, "fstype"),
        mountpoint: text_field(value, "mountpoint"),
        model: text_field(value, "model").trim().to_string(),
        label: text_field(value, "label"),
        children: value
            .get("children")
            .and_then(|field| field.as_array())
            .map(|children| children.iter().filter_map(parse_device).collect())
            .unwrap_or_default(),
    };
    Some(device)
}

/// Analyse `/etc/fstab` (commentaires et lignes vides ignorés).
pub fn parse_fstab(text: &str) -> Vec<FstabEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.split('#').next()?.trim();
            if line.is_empty() {
                return None;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 3 {
                return None;
            }
            Some(FstabEntry {
                source: fields[0].to_string(),
                target: fields[1].to_string(),
                fstype: fields[2].to_string(),
                options: fields.get(3).unwrap_or(&"defaults").to_string(),
            })
        })
        .collect()
}

/// Analyse la sortie JSON de `findmnt`.
pub fn parse_findmnt(text: &str) -> Vec<MountInfo> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(filesystems) = value.get("filesystems").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    filesystems
        .iter()
        .map(|entry| MountInfo {
            target: text_field(entry, "target"),
            source: text_field(entry, "source"),
            fstype: text_field(entry, "fstype"),
            options: text_field(entry, "options"),
        })
        .collect()
}

/// Analyse `timeshift --list`.
pub fn parse_timeshift(text: &str) -> Vec<Snapshot> {
    let mut snapshots = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Num") || trimmed.starts_with('-') {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        let Some(name) = fields
            .iter()
            .find(|field| {
                field.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                    && field.contains('_')
                    && field.contains('-')
            })
        else {
            continue;
        };
        let tags = fields
            .iter()
            .find(|field| {
                *field != name
                    && field.len() <= 3
                    && field.chars().all(|c| c.is_ascii_uppercase())
            })
            .copied()
            .unwrap_or("");
        snapshots.push(Snapshot {
            name: name.to_string(),
            tags: tags.to_string(),
            created: name.replace('_', " "),
        });
    }
    snapshots
}

/// Lit l'ensemble de l'état du stockage (lecture seule).
pub fn report() -> StorageReport {
    let mut report = StorageReport::default();

    match exec::run_with_timeout(
        "lsblk",
        &[
            "-J",
            "-b",
            "-o",
            "NAME,PATH,TYPE,SIZE,FSTYPE,MOUNTPOINT,MODEL,LABEL",
        ],
        std::time::Duration::from_secs(20),
    ) {
        Ok(captured) if captured.ok => {
            for device in parse_lsblk(&captured.stdout) {
                device.flatten(0, &mut report.devices);
            }
        }
        Ok(captured) => report.errors.push(captured.error_text()),
        Err(error) => report.errors.push(error.0),
    }

    if exec::exists("findmnt") {
        if let Ok(captured) = exec::run_with_timeout(
            "findmnt",
            &["-J", "-b", "-o", "TARGET,SOURCE,FSTYPE,OPTIONS", "--real"],
            std::time::Duration::from_secs(20),
        ) {
            report.mounts = parse_findmnt(&captured.stdout);
        }
    }

    report.fstab = std::fs::read_to_string("/etc/fstab")
        .map(|text| parse_fstab(&text))
        .unwrap_or_default();

    // Btrfs : uniquement pour les montages réellement en btrfs.
    if exec::exists("btrfs") {
        let btrfs_mounts: Vec<String> = report
            .mounts
            .iter()
            .filter(|mount| mount.fstype == "btrfs")
            .map(|mount| mount.target.clone())
            .collect();
        for target in btrfs_mounts {
            if let Ok(captured) = exec::run_with_timeout(
                "btrfs",
                &["filesystem", "usage", "-b", &target],
                std::time::Duration::from_secs(20),
            ) {
                for line in captured.stdout.lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        report.btrfs.push(line.to_string());
                    }
                }
            }
        }
    }

    if exec::exists("timeshift") {
        match exec::run_with_timeout(
            "timeshift",
            &["--list"],
            std::time::Duration::from_secs(30),
        ) {
            Ok(captured) => {
                if captured.ok {
                    report.snapshots = parse_timeshift(&captured.stdout);
                } else if !captured.stderr.trim().is_empty() {
                    report.errors.push(format!("Timeshift : {}", captured.error_text()));
                }
            }
            Err(error) => report.errors.push(format!("Timeshift : {}", error.0)),
        }
    }

    report
}

/// Dossiers techniques ignorés par la recherche de doublons.
const SKIPPED: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".cache",
    "snap",
    "proc",
    "sys",
    "dev",
];

fn hash_prefix(path: &Path, limit: usize) -> Option<u64> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buffer = vec![0u8; limit];
    let read = file.read(&mut buffer).ok()?;
    let mut hasher = DefaultHasher::new();
    hasher.write(&buffer[..read]);
    Some(hasher.finish())
}

/// Recherche les fichiers identiques sous `root` (taille puis empreinte).
pub fn find_duplicates(root: &Path, min_size: u64, max_files: usize) -> DuplicateScan {
    let mut scan = DuplicateScan {
        root: root.display().to_string(),
        ..Default::default()
    };
    if !root.is_dir() {
        return scan;
    }
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut stack = vec![root.to_path_buf()];

    'walk: while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if scan.files_scanned >= max_files {
                scan.truncated = true;
                break 'walk;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !SKIPPED.contains(&name.as_str()) {
                    stack.push(entry.path());
                }
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() || metadata.len() < min_size.max(1) {
                continue;
            }
            scan.files_scanned += 1;
            by_size.entry(metadata.len()).or_default().push(entry.path());
        }
    }

    let mut groups = Vec::new();
    for (size, paths) in by_size.into_iter() {
        if paths.len() < 2 {
            continue;
        }
        let mut by_fingerprint: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if let Some(fingerprint) = hash_prefix(&path, 64 * 1024) {
                by_fingerprint.entry(fingerprint).or_default().push(path);
            }
        }
        for (fingerprint, paths) in by_fingerprint {
            if paths.len() < 2 {
                continue;
            }
            scan.bytes_duplicated += size * (paths.len() as u64 - 1);
            groups.push(DuplicateGroup {
                size,
                fingerprint: format!("{fingerprint:016x}"),
                paths,
            });
        }
    }
    groups.sort_by(|a, b| {
        b.size
            .saturating_mul(b.paths.len() as u64)
            .cmp(&a.size.saturating_mul(a.paths.len() as u64))
    });
    groups.truncate(500);
    scan.groups = groups;
    scan
}

/// Texte lisible pour un groupe de doublons.
pub fn describe_group(group: &DuplicateGroup) -> String {
    format!(
        "{} × {} ({} récupérables)",
        group.paths.len(),
        util::human_bytes(group.size),
        util::human_bytes(group.size * (group.paths.len() as u64 - 1))
    )
}

/// Supprime les copies d'un groupe de doublons en gardant la première.
/// Chaque copie doit rester sous `root`, être un fichier ordinaire (pas un lien
/// symbolique) et avoir toujours la taille du groupe ; sinon elle est conservée.
pub fn delete_duplicates(group: &DuplicateGroup, root: &Path) -> Result<(usize, u64), String> {
    let Some(original) = group.paths.first() else {
        return Err("groupe vide".to_string());
    };
    let original_meta = std::fs::symlink_metadata(original)
        .map_err(|error| format!("original introuvable : {error}"))?;
    if !original_meta.is_file() || original_meta.len() != group.size {
        return Err("l'original a changé depuis l'analyse : relancez l'analyse".to_string());
    }
    let root = crate::util::normalize(root);
    let mut removed = 0usize;
    let mut freed = 0u64;
    for path in group.paths.iter().skip(1) {
        if !crate::util::normalize(path).starts_with(&root) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            continue;
        };
        if !meta.is_file() || meta.len() != group.size {
            continue;
        }
        if std::fs::remove_file(path).is_ok() {
            removed += 1;
            freed += meta.len();
        }
    }
    Ok((removed, freed))
}

/// Export CSV d'une analyse de doublons.
pub fn duplicates_csv(scan: &DuplicateScan) -> String {
    let mut out = String::from("taille_octets;copies;fichiers\n");
    for group in &scan.groups {
        let files: Vec<String> = group
            .paths
            .iter()
            .map(|path| path.display().to_string().replace(';', ",").replace('\n', " "))
            .collect();
        out.push_str(&format!("{};{};{}\n", group.size, group.paths.len(), files.join(" | ")));
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lsblk_tree() {
        let json = r#"{"blockdevices":[
            {"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","size":500107862016,
             "fstype":null,"mountpoint":null,"model":"Samsung SSD","label":null,
             "children":[{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","type":"part",
                          "size":499963174912,"fstype":"ext4","mountpoint":"/"}]}]}"#;
        let devices = parse_lsblk(json);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].size, 500107862016);
        assert_eq!(devices[0].children.len(), 1);
        assert_eq!(devices[0].children[0].fstype, "ext4");
        assert_eq!(devices[0].children[0].mountpoint, "/");
        let mut flat = Vec::new();
        devices[0].flatten(0, &mut flat);
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[1].0, 1);
    }

    #[test]
    fn parses_fstab_with_comments() {
        let text = "# /etc/fstab\nUUID=abcd / ext4 defaults 0 1\n\n# commentaire\n/dev/sdb1 /mnt/data btrfs defaults,noatime 0 2\n";
        let entries = parse_fstab(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].fstype, "btrfs");
        assert_eq!(entries[1].options, "defaults,noatime");
    }

    #[test]
    fn parses_findmnt_output() {
        let json = r#"{"filesystems":[
            {"target":"/","source":"/dev/nvme0n1p2","fstype":"ext4","options":"rw,relatime"},
            {"target":"/boot/efi","source":"/dev/nvme0n1p1","fstype":"vfat","options":"rw"}]}"#;
        let mounts = parse_findmnt(json);
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].target, "/");
        assert_eq!(mounts[1].fstype, "vfat");
    }

    #[test]
    fn parses_timeshift_listing() {
        let text = "Num     Name                 Tags  Description\n------------------------------------------------------------------------------\n0    >  2026-11-01_10-00-00  O     Before upgrade\n1    >  2026-10-01_09-00-00  B     Boot\n";
        let snapshots = parse_timeshift(text);
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].name, "2026-11-01_10-00-00");
        assert_eq!(snapshots[0].tags, "O");
    }

    #[test]
    fn finds_identical_files() {
        let root = std::env::temp_dir().join(format!("um-dupes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        let content = b"contenu identique pour le test".repeat(64);
        std::fs::write(root.join("a/fichier.txt"), &content).unwrap();
        std::fs::write(root.join("b/copie.txt"), &content).unwrap();
        std::fs::write(root.join("b/autre.txt"), b"different").unwrap();
        let scan = find_duplicates(&root, 10, 10_000);
        assert_eq!(scan.groups.len(), 1);
        assert_eq!(scan.groups[0].paths.len(), 2);
        assert!(scan.bytes_duplicated > 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn report_collects_without_panicking() {
        let report = report();
        assert!(report.errors.len() < 10);
    }

    #[test]
    fn skips_small_files() {
        let root = std::env::temp_dir().join(format!("um-dupes-small-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), b"x").unwrap();
        std::fs::write(root.join("b.txt"), b"x").unwrap();
        let scan = find_duplicates(&root, 1024, 1_000);
        assert!(scan.groups.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deletes_copies_but_keeps_the_original() {
        let root = std::env::temp_dir().join(format!("um-dupdel-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let (a, b, c) = (root.join("a.txt"), root.join("b.txt"), root.join("c.txt"));
        for file in [&a, &b, &c] {
            std::fs::write(file, "même contenu").unwrap();
        }
        let size = std::fs::metadata(&a).unwrap().len();
        let group = DuplicateGroup { size, fingerprint: "x".into(), paths: vec![a.clone(), b.clone(), c.clone()] };
        let (removed, freed) = delete_duplicates(&group, &root).unwrap();
        assert_eq!((removed, freed), (2, size * 2));
        assert!(a.exists() && !b.exists() && !c.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn keeps_files_that_changed_or_left_the_root() {
        let root = std::env::temp_dir().join(format!("um-dupkeep-{}", std::process::id()));
        let other = std::env::temp_dir().join(format!("um-dupkeep-ext-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let original = root.join("o.txt");
        let changed = root.join("changed.txt");
        let outside = other.join("outside.txt");
        let link = root.join("link.txt");
        std::fs::write(&original, "abc").unwrap();
        std::fs::write(&changed, "abcdef").unwrap();
        std::fs::write(&outside, "abc").unwrap();
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let group = DuplicateGroup {
            size: 3,
            fingerprint: "x".into(),
            paths: vec![original.clone(), changed.clone(), outside.clone(), link.clone()],
        };
        let (removed, _) = delete_duplicates(&group, &root).unwrap();
        assert_eq!(removed, 0);
        assert!(changed.exists() && outside.exists());
        assert!(std::fs::symlink_metadata(&link).is_ok(), "un lien symbolique n'est jamais supprimé");
        // L'original modifié bloque tout le groupe.
        std::fs::write(&original, "plus long").unwrap();
        assert!(delete_duplicates(&group, &root).is_err());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn exports_duplicates_as_csv() {
        let scan = DuplicateScan {
            groups: vec![DuplicateGroup { size: 10, fingerprint: "f".into(), paths: vec![PathBuf::from("/a;b"), PathBuf::from("/c")] }],
            ..DuplicateScan::default()
        };
        let csv = duplicates_csv(&scan);
        assert!(csv.starts_with("taille_octets;copies;fichiers\n"));
        assert!(csv.contains("10;2;/a,b | /c"));
    }
}
