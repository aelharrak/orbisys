//! Stockage : nettoyage avancé, montages, clé USB bootable, archives.

use crate::disk;
use crate::exec;
use crate::panel::*;
use crate::util;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Nettoyage avancé
// ---------------------------------------------------------------------------

/// Caches applicatifs nettoyables : `(clé, libellé, chemin relatif au dossier personnel)`.
pub const CACHE_TARGETS: [(&str, &str, &str); 7] = [
    ("firefox", "Cache Firefox", ".cache/mozilla"),
    ("chrome", "Cache Chrome", ".cache/google-chrome"),
    ("chromium", "Cache Chromium", ".cache/chromium"),
    ("brave", "Cache Brave", ".cache/BraveSoftware"),
    ("thumbnails", "Vignettes", ".cache/thumbnails"),
    ("pip", "Cache pip", ".cache/pip"),
    ("apt-user", "Cache APT utilisateur", ".cache/apt"),
];

/// Chemin absolu d'une cible de nettoyage connue.
pub fn cache_path(key: &str) -> Option<PathBuf> {
    CACHE_TARGETS
        .iter()
        .find(|(name, _, _)| *name == key)
        .map(|(_, _, relative)| home().join(relative))
}

/// Vide le contenu d'un cache connu (le dossier lui-même est conservé).
fn clear_cache(args: &[String]) -> Result<String, String> {
    let [key] = args else {
        return Err("cible attendue".to_string());
    };
    let path = cache_path(key).ok_or_else(|| format!("cible inconnue : {key}"))?;
    if !util::path_within(&home().join(".cache"), &path) {
        return Err("chemin hors du cache utilisateur".to_string());
    }
    let mut removed = 0u64;
    let Ok(entries) = std::fs::read_dir(&path) else {
        return Ok("Rien à nettoyer".to_string());
    };
    for entry in entries.flatten() {
        let child = entry.path();
        // Les liens symboliques sont supprimés comme tels, jamais suivis.
        let result = match entry.file_type() {
            Ok(kind) if kind.is_dir() => std::fs::remove_dir_all(&child),
            Ok(_) => std::fs::remove_file(&child),
            Err(error) => Err(error),
        };
        if result.is_ok() {
            removed += 1;
        }
    }
    Ok(format!("{removed} élément(s) supprimé(s)"))
}

/// Révisions Snap désactivées : `(nom, révision)`.
pub fn parse_snap_disabled(text: &str) -> Vec<(String, String)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 4 || !line.contains("disabled") {
                return None;
            }
            let revision = fields[2];
            if !revision.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            Some((fields[0].to_string(), revision.to_string()))
        })
        .collect()
}

pub fn advanced_cleanup() -> Panel {
    let mut panel = Panel::default();
    let mut rows = Vec::new();
    let mut total = 0u64;
    for (key, label, _) in CACHE_TARGETS {
        let Some(path) = cache_path(key) else { continue };
        if !path.exists() {
            continue;
        }
        let size = util::dir_size(&path, 200_000);
        total += size;
        rows.push(
            Row::owned(vec![label.to_string(), util::human_bytes(size), path.display().to_string()]).action(
                Cmd::local("Nettoyer", clear_cache, vec![key.to_string()])
                    .confirm(&format!("Vider {label} ({}) ?", util::human_bytes(size))),
            ),
        );
    }
    panel.metric("Caches récupérables", util::human_bytes(total), Level::Info);
    panel.section(
        Section::new("Caches applicatifs")
            .columns(&["Cache", "Taille", "Dossier"])
            .rows(rows),
    );

    let mut tools = Section::new("Autres nettoyages").note("Chaque action est confirmée ; celles qui touchent au système passent par Polkit.");
    if exec::exists("flatpak") {
        tools = tools.action(
            Cmd::new("Supprimer les runtimes Flatpak inutilisés", "flatpak", &["uninstall", "--unused", "-y"])
                .confirm("Supprimer les runtimes Flatpak inutilisés ?"),
        );
    }
    if let Some(engine) = ["docker", "podman"].into_iter().find(|name| exec::exists(name)) {
        let usage = run_lossy(engine, &["system", "df"]);
        tools = tools.action(
            Cmd::new("Nettoyer les données conteneurs", engine, &["system", "prune", "-f"])
                .confirm(&format!("Supprimer conteneurs arrêtés, réseaux et images orphelines ?\n{usage}")),
        );
    }
    panel.section(tools);

    if exec::exists("snap") {
        let disabled = parse_snap_disabled(&run_lossy("snap", &["list", "--all"]));
        let rows = disabled
            .iter()
            .filter(|(name, _)| util::valid_package_name(name))
            .map(|(name, revision)| {
                Row::new(&[name, revision]).action(
                    Cmd::new(
                        "Supprimer",
                        "snap",
                        &["remove", name, &format!("--revision={revision}")],
                    )
                    .privileged()
                    .confirm(&format!("Supprimer la révision {revision} de {name} ?")),
                )
            })
            .collect();
        panel.section(
            Section::new("Anciennes révisions Snap")
                .columns(&["Snap", "Révision"])
                .rows(rows),
        );
    }
    panel
}

// ---------------------------------------------------------------------------
// Montages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct FstabEntry {
    pub device: String,
    pub mount: String,
    pub fstype: String,
    pub options: String,
}

pub fn parse_fstab(text: &str) -> Vec<FstabEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 4 {
                return None;
            }
            Some(FstabEntry {
                device: fields[0].to_string(),
                mount: fields[1].to_string(),
                fstype: fields[2].to_string(),
                options: fields[3].to_string(),
            })
        })
        .collect()
}

/// Seuls les montages amovibles ou temporaires peuvent être démontés d'ici.
pub fn unmountable(target: &str) -> bool {
    safe_abs_path(target)
        && ["/media/", "/mnt/", "/run/media/"]
            .iter()
            .any(|prefix| target.starts_with(prefix))
}

pub fn mounts_panel() -> Panel {
    let mut panel = Panel::default();
    let mounts = disk::mounts();
    panel.metric("Montages actifs", mounts.len().to_string(), Level::Info);
    let rows = mounts
        .iter()
        .map(|mount| {
            let mut row = Row::owned(vec![
                mount.target.clone(),
                mount.source.clone(),
                mount.fstype.clone(),
                util::human_bytes(mount.total),
                format!("{:.0} %", mount.percent),
            ])
            .bad(mount.critical());
            if unmountable(&mount.target) {
                row = row.action(
                    Cmd::new("Démonter", "umount", &["--", &mount.target])
                        .privileged()
                        .confirm(&format!("Démonter {} ?", mount.target)),
                );
            }
            row
        })
        .collect();
    panel.section(
        Section::new("Points de montage actifs")
            .chart(
                Chart::bars(
                    "Occupation des systèmes de fichiers (trait : 90 %)",
                    "%",
                    mounts.iter().map(|m| m.target.clone()).collect(),
                    mounts.iter().map(|m| m.percent).collect(),
                )
                .max_y(100.0)
                .marks(vec![90.0; mounts.len()]),
            )
            .columns(&["Montage", "Source", "Type", "Taille", "Occupation"])
            .rows(rows)
            .action(
                Cmd::new("TRIM des SSD (fstrim)", "fstrim", &["-av"])
                    .privileged()
                    .confirm("Lancer fstrim -av sur les systèmes de fichiers compatibles ?"),
            ),
    );
    let fstab = parse_fstab(&read("/etc/fstab"));
    let rows = fstab
        .iter()
        .map(|entry| {
            Row::owned(vec![
                entry.mount.clone(),
                entry.device.clone(),
                entry.fstype.clone(),
                util::ellipsis(&entry.options, 50),
            ])
        })
        .collect();
    panel.section(
        Section::new("/etc/fstab (lecture seule)")
            .columns(&["Montage", "Périphérique", "Type", "Options"])
            .rows(rows),
    );
    panel
}

// ---------------------------------------------------------------------------
// Clé USB bootable
// ---------------------------------------------------------------------------

/// Un disque tel que décrit par `lsblk -P`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDevice {
    pub name: String,
    pub size: String,
    pub kind: String,
    pub removable: bool,
    pub model: String,
    pub transport: String,
    pub mountpoint: String,
}

/// Analyse `lsblk -P -o NAME,SIZE,RM,TYPE,MODEL,TRAN,MOUNTPOINT`.
pub fn parse_lsblk_pairs(text: &str) -> Vec<BlockDevice> {
    text.lines()
        .filter(|line| line.contains("NAME="))
        .map(|line| {
            let mut device = BlockDevice::default();
            let mut rest = line;
            while let Some(eq) = rest.find("=\"") {
                let key = rest[..eq].trim();
                let after = &rest[eq + 2..];
                let Some(end) = after.find('"') else { break };
                let value = &after[..end];
                match key {
                    "NAME" => device.name = value.to_string(),
                    "SIZE" => device.size = value.to_string(),
                    "RM" => device.removable = value == "1",
                    "TYPE" => device.kind = value.to_string(),
                    "MODEL" => device.model = value.trim().to_string(),
                    "TRAN" => device.transport = value.to_string(),
                    "MOUNTPOINT" => device.mountpoint = value.to_string(),
                    _ => {}
                }
                rest = &after[end + 1..];
            }
            device
        })
        .collect()
}

pub fn block_devices() -> Vec<BlockDevice> {
    parse_lsblk_pairs(&run_lossy(
        "lsblk",
        &["-P", "-o", "NAME,SIZE,RM,TYPE,MODEL,TRAN,MOUNTPOINT"],
    ))
}

/// Disques amovibles entiers (clé USB, carte SD), jamais le disque système.
pub fn removable_disks(devices: &[BlockDevice]) -> Vec<&BlockDevice> {
    devices
        .iter()
        .filter(|device| {
            device.kind == "disk"
                && (device.removable || device.transport == "usb")
                && !devices
                    .iter()
                    .any(|other| other.name.starts_with(&device.name) && matches!(other.mountpoint.as_str(), "/" | "/boot" | "/boot/efi" | "/home"))
        })
        .collect()
}

fn build_usb(values: &[String]) -> Result<Cmd, String> {
    let [iso, device, confirm] = values else {
        return Err("image, périphérique et confirmation attendus".to_string());
    };
    let (iso, device, confirm) = (iso.trim(), device.trim(), confirm.trim());
    if !safe_abs_path(iso) {
        return Err("chemin d'image invalide (absolu requis)".to_string());
    }
    let lower = iso.to_ascii_lowercase();
    if !(lower.ends_with(".iso") || lower.ends_with(".img")) {
        return Err("l'image doit être un fichier .iso ou .img".to_string());
    }
    if !Path::new(iso).is_file() {
        return Err(format!("image introuvable : {iso}"));
    }
    let Some(name) = device.strip_prefix("/dev/") else {
        return Err("périphérique attendu sous la forme /dev/sdX".to_string());
    };
    if confirm != device {
        return Err("retapez exactement le périphérique pour confirmer".to_string());
    }
    let devices = block_devices();
    let allowed = removable_disks(&devices);
    let Some(target) = allowed.iter().find(|candidate| candidate.name == name) else {
        return Err(format!("{device} n'est pas un disque amovible reconnu"));
    };
    Ok(Cmd::local(
        "Écriture de l'image",
        write_image,
        vec![iso.to_string(), device.to_string()],
    )
    .confirm(&format!(
        "TOUTES les données de {device} ({} {}) seront EFFACÉES.\nImage : {iso}\nContinuer ?",
        target.size, target.model
    )))
}

/// Écrit l'image après une dernière vérification : le périphérique doit encore
/// être un disque amovible hors système (la clé a pu être remplacée pendant
/// la confirmation).
fn write_image(args: &[String]) -> Result<String, String> {
    let [iso, device] = args else {
        return Err("image et périphérique attendus".to_string());
    };
    let name = device.strip_prefix("/dev/").ok_or("périphérique invalide")?;
    if !safe_abs_path(iso) || !Path::new(iso).is_file() {
        return Err("image introuvable".to_string());
    }
    let devices = block_devices();
    if !removable_disks(&devices).iter().any(|candidate| candidate.name == name) {
        return Err(format!("{device} n'est plus un disque amovible reconnu : opération annulée"));
    }
    Cmd::new(
        "Écriture de l'image",
        "dd",
        &[&format!("if={iso}"), &format!("of={device}"), "bs=4M", "conv=fsync", "status=none"],
    )
    .privileged()
    .run()
}

pub fn usb_bootable() -> Panel {
    let mut panel = Panel::default();
    let devices = block_devices();
    let removable = removable_disks(&devices);
    panel.metric("Périphériques amovibles", removable.len().to_string(), Level::Info);
    let rows = removable
        .iter()
        .map(|device| {
            Row::owned(vec![
                format!("/dev/{}", device.name),
                device.size.clone(),
                device.model.clone(),
                device.transport.clone(),
            ])
        })
        .collect();
    panel.section(
        Section::new("Disques amovibles détectés")
            .columns(&["Périphérique", "Taille", "Modèle", "Bus"])
            .rows(rows)
            .note("Les disques contenant le système (/, /boot, /home) ne sont jamais proposés."),
    );
    panel.section(
        Section::new("Écrire une image")
            .note("Opération destructrice : le périphérique cible est entièrement écrasé. Retapez son nom pour confirmer.")
            .form(Form {
                fields: vec![
                    Field::new("Image ISO", "/chemin/image.iso"),
                    Field::new("Périphérique", "/dev/sdX"),
                    Field::new("Confirmation", "retapez le périphérique"),
                ],
                submit: "Écrire sur la clé".to_string(),
                build: build_usb,
            }),
    );
    panel
}

// ---------------------------------------------------------------------------
// Archives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    Zip,
    SevenZip,
}

pub fn archive_kind(path: &str) -> Option<ArchiveKind> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Some(ArchiveKind::TarGz)
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        Some(ArchiveKind::TarXz)
    } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
        Some(ArchiveKind::TarBz2)
    } else if lower.ends_with(".tar") {
        Some(ArchiveKind::Tar)
    } else if lower.ends_with(".zip") {
        Some(ArchiveKind::Zip)
    } else if lower.ends_with(".7z") {
        Some(ArchiveKind::SevenZip)
    } else {
        None
    }
}

/// Dossier d'extraction : à côté de l'archive, nommé comme elle sans extension.
pub fn extraction_dir(archive: &str) -> Option<PathBuf> {
    let path = Path::new(archive);
    let name = path.file_name()?.to_string_lossy().to_string();
    let lower = name.to_ascii_lowercase();
    let stem = [".tar.gz", ".tar.xz", ".tar.bz2", ".tgz", ".txz", ".tbz2", ".tar", ".zip", ".7z"]
        .iter()
        .find_map(|suffix| lower.ends_with(suffix).then(|| name[..name.len() - suffix.len()].to_string()))?;
    if stem.is_empty() || stem.starts_with('.') {
        return None;
    }
    Some(path.parent()?.join(stem))
}

/// Un nom de membre d'archive est-il dangereux (absolu ou avec `..`) ?
pub fn unsafe_member(name: &str) -> bool {
    let name = name.trim();
    name.starts_with('/')
        || name.starts_with('~')
        || name.split(['/', '\\']).any(|part| part == "..")
        || name.chars().any(|ch| ch.is_control())
}

/// Liste les membres d'une archive avant extraction (tar et zip).
fn list_members(kind: ArchiveKind, archive: &str) -> Result<Vec<String>, String> {
    let (program, args): (&str, Vec<&str>) = match kind {
        ArchiveKind::Zip => ("unzip", vec!["-Z1", archive]),
        ArchiveKind::SevenZip => return Ok(Vec::new()),
        _ => ("tar", vec!["-tf", archive]),
    };
    let text = run_text(program, &args)?;
    Ok(text.lines().map(str::to_string).collect())
}

fn extract_archive(args: &[String]) -> Result<String, String> {
    let [archive] = args else {
        return Err("archive attendue".to_string());
    };
    if !safe_abs_path(archive) || !Path::new(archive).is_file() {
        return Err(format!("archive invalide : {archive}"));
    }
    let kind = archive_kind(archive).ok_or("format non pris en charge")?;
    let dest = extraction_dir(archive).ok_or("nom d'archive invalide")?;
    if let Some(bad) = list_members(kind, archive)?.iter().find(|name| unsafe_member(name)) {
        return Err(format!("archive refusée : membre dangereux « {bad} »"));
    }
    std::fs::create_dir_all(&dest).map_err(|error| error.to_string())?;
    let dest_text = dest.to_string_lossy().to_string();
    let command = match kind {
        ArchiveKind::Zip => ("unzip", vec!["-q".to_string(), "-o".into(), archive.clone(), "-d".into(), dest_text.clone()]),
        ArchiveKind::SevenZip => ("7z", vec!["x".to_string(), "-y".into(), format!("-o{dest_text}"), archive.clone()]),
        _ => ("tar", vec!["-xf".to_string(), archive.clone(), "-C".into(), dest_text.clone()]),
    };
    Cmd::owned("Extraction", command.0, command.1).run()?;
    Ok(format!("Extrait dans {dest_text}"))
}

fn build_archive(values: &[String]) -> Result<Cmd, String> {
    let [source, target] = values else {
        return Err("dossier et archive attendus".to_string());
    };
    let (source, target) = (source.trim(), target.trim());
    if !safe_abs_path(source) || !Path::new(source).is_dir() {
        return Err("le dossier source doit exister (chemin absolu)".to_string());
    }
    if !safe_abs_path(target) {
        return Err("chemin d'archive invalide".to_string());
    }
    let flag = match archive_kind(target) {
        Some(ArchiveKind::Tar) => "-cf",
        Some(ArchiveKind::TarGz) => "-czf",
        Some(ArchiveKind::TarXz) => "-cJf",
        Some(ArchiveKind::TarBz2) => "-cjf",
        _ => return Err("créez une archive .tar, .tar.gz, .tar.xz ou .tar.bz2".to_string()),
    };
    if Path::new(target).exists() {
        return Err("l'archive existe déjà".to_string());
    }
    let src = Path::new(source);
    let parent = src.parent().ok_or("dossier source sans parent")?;
    let name = src.file_name().ok_or("dossier source invalide")?.to_string_lossy().to_string();
    Ok(Cmd::owned(
        "Création de l'archive",
        "tar",
        vec![flag.to_string(), target.to_string(), "-C".into(), parent.to_string_lossy().to_string(), "--".into(), name],
    ))
}

/// Archives d'un dossier, une profondeur.
pub fn list_archives(dir: &Path) -> Vec<(PathBuf, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, u64)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let meta = entry.metadata().ok()?;
            (meta.is_file() && archive_kind(&path.to_string_lossy()).is_some()).then_some((path, meta.len()))
        })
        .collect();
    found.sort();
    found
}

pub fn archives() -> Panel {
    let mut panel = Panel::default();
    let mut dirs = vec![home()];
    if let Some(downloads) = dirs::download_dir() {
        if downloads != home() {
            dirs.push(downloads);
        }
    }
    let mut rows = Vec::new();
    for dir in &dirs {
        for (path, size) in list_archives(dir) {
            let text = path.to_string_lossy().to_string();
            rows.push(
                Row::owned(vec![
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    util::human_bytes(size),
                    dir.display().to_string(),
                ])
                .action(
                    Cmd::local("Extraire", extract_archive, vec![text.clone()])
                        .confirm(&format!("Extraire {text} à côté de l'archive ?")),
                ),
            );
        }
    }
    panel.metric("Archives trouvées", rows.len().to_string(), Level::Info);
    panel.section(
        Section::new("Archives (dossier personnel et Téléchargements)")
            .columns(&["Archive", "Taille", "Dossier"])
            .rows(rows),
    );
    panel.section(
        Section::new("Créer une archive")
            .note("Formats : .tar, .tar.gz, .tar.xz, .tar.bz2. L'archive ne doit pas exister.")
            .form(Form {
                fields: vec![
                    Field::new("Dossier", "/chemin/dossier"),
                    Field::new("Archive", "/chemin/archive.tar.gz"),
                ],
                submit: "Créer".to_string(),
                build: build_archive,
            }),
    );
    panel
}

// ---------------------------------------------------------------------------
// Btrfs : scrub, équilibrage, usage
// ---------------------------------------------------------------------------

/// Montage btrfs connu et de forme sûre.
pub fn btrfs_target_ok(mounts: &[String], path: &str) -> bool {
    crate::btrfs::valid_path(path) && mounts.iter().any(|known| known == path)
}

pub fn btrfs_maintenance() -> Panel {
    let mut panel = Panel::default();
    if !exec::exists("btrfs") {
        panel.error("btrfs-progs absent : installez le paquet « btrfs-progs ».");
    }
    let mounts: Vec<String> = crate::btrfs::parse_mounts(&read("/proc/mounts"))
        .into_iter()
        .map(|mount| mount.target)
        .collect();
    panel.metric("Systèmes btrfs", mounts.len().to_string(), Level::Info);
    let rows = mounts
        .iter()
        .filter(|path| btrfs_target_ok(&mounts, path))
        .map(|path| {
            Row::new(&[path])
                .action(Cmd::new("Lancer un scrub", "btrfs", &["scrub", "start", path]).privileged()
                    .confirm(&format!("Vérifier l'intégrité des données de {path} (scrub) ? L'opération tourne en arrière-plan.")))
                .action(Cmd::new("État du scrub", "btrfs", &["scrub", "status", path]).privileged())
                .action(Cmd::new("Usage", "btrfs", &["filesystem", "usage", path]).privileged())
                .action(Cmd::new("Équilibrer (50 %)", "btrfs", &["balance", "start", "-dusage=50", "-musage=50", path]).privileged()
                    .confirm(&format!("Rééquilibrer les blocs de {path} (usage < 50 %) ? L'opération peut être longue et charger le disque.")))
        })
        .collect();
    panel.section(
        Section::new("Montages btrfs")
            .columns(&["Montage"])
            .rows(rows)
            .note("Les instantanés et sous-volumes se gèrent dans l'écran « Btrfs ». Les résultats s'affichent dans la zone de sortie."),
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_targets_stay_inside_user_cache() {
        for (key, _, _) in CACHE_TARGETS {
            let path = cache_path(key).unwrap();
            assert!(util::path_within(&home().join(".cache"), &path), "{key}");
        }
        assert!(cache_path("../../etc").is_none());
        assert!(clear_cache(&["inconnu".into()]).is_err());
        assert!(clear_cache(&[]).is_err());
    }

    #[test]
    fn parses_disabled_snaps() {
        let text = "Name Version Rev Tracking Publisher Notes\ncore22 20240111 1380 latest/stable canonical** base\ncore22 20231123 1033 latest/stable canonical** base,disabled\nfirefox 1 99 latest/stable mozilla** disabled\n";
        let parsed = parse_snap_disabled(text);
        assert_eq!(parsed, vec![("core22".to_string(), "1033".to_string()), ("firefox".to_string(), "99".to_string())]);
    }

    #[test]
    fn parses_fstab() {
        let text = "# commentaire\nUUID=abc / ext4 errors=remount-ro 0 1\n\n/swapfile none swap sw 0 0\n";
        let entries = parse_fstab(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].mount, "/");
        assert_eq!(entries[1].fstype, "swap");
    }

    #[test]
    fn only_removable_mounts_can_be_unmounted() {
        assert!(unmountable("/media/amine/USB"));
        assert!(unmountable("/run/media/amine/USB"));
        assert!(!unmountable("/"));
        assert!(!unmountable("/home"));
        assert!(!unmountable("/media/../etc"));
    }

    const LSBLK: &str = "NAME=\"sda\" SIZE=\"500G\" RM=\"0\" TYPE=\"disk\" MODEL=\"Samsung SSD\" TRAN=\"sata\" MOUNTPOINT=\"\"\nNAME=\"sda1\" SIZE=\"500G\" RM=\"0\" TYPE=\"part\" MODEL=\"\" TRAN=\"\" MOUNTPOINT=\"/\"\nNAME=\"sdb\" SIZE=\"16G\" RM=\"1\" TYPE=\"disk\" MODEL=\"USB Flash\" TRAN=\"usb\" MOUNTPOINT=\"\"\nNAME=\"sdb1\" SIZE=\"16G\" RM=\"1\" TYPE=\"part\" MODEL=\"\" TRAN=\"\" MOUNTPOINT=\"/media/u/KEY\"\n";

    #[test]
    fn selects_only_removable_non_system_disks() {
        let devices = parse_lsblk_pairs(LSBLK);
        assert_eq!(devices.len(), 4);
        let removable = removable_disks(&devices);
        assert_eq!(removable.len(), 1);
        assert_eq!(removable[0].name, "sdb");
        assert_eq!(removable[0].model, "USB Flash");
    }

    #[test]
    fn usb_form_rejects_unsafe_input() {
        let base = |iso: &str, dev: &str, confirm: &str| build_usb(&[iso.into(), dev.into(), confirm.into()]);
        assert!(base("relatif.iso", "/dev/sdb", "/dev/sdb").is_err());
        assert!(base("/tmp/x.txt", "/dev/sdb", "/dev/sdb").is_err());
        assert!(base("/tmp/inexistant.iso", "/dev/sdb", "/dev/sdb").is_err());
        let iso = std::env::temp_dir().join("um-test.iso");
        std::fs::write(&iso, b"x").unwrap();
        let iso = iso.to_string_lossy().to_string();
        assert!(base(&iso, "sdb", "sdb").is_err());
        assert!(base(&iso, "/dev/sda", "/dev/sdb").is_err());
        assert!(base(&iso, "/dev/sda;rm", "/dev/sda;rm").is_err());
        let _ = std::fs::remove_file(&iso);
    }

    #[test]
    fn detects_archive_kinds_and_dirs() {
        assert_eq!(archive_kind("/a/b.tar.gz"), Some(ArchiveKind::TarGz));
        assert_eq!(archive_kind("/a/b.ZIP"), Some(ArchiveKind::Zip));
        assert_eq!(archive_kind("/a/b.7z"), Some(ArchiveKind::SevenZip));
        assert_eq!(archive_kind("/a/b.txt"), None);
        assert_eq!(extraction_dir("/home/u/projet.tar.gz"), Some(PathBuf::from("/home/u/projet")));
        assert_eq!(extraction_dir("/home/u/.zip"), None);
    }

    #[test]
    fn archive_form_validates_paths() {
        let dir = std::env::temp_dir().to_string_lossy().to_string();
        assert!(build_archive(&["relatif".into(), "/tmp/a.tar".into()]).is_err());
        assert!(build_archive(&[dir.clone(), "/tmp/a.exe".into()]).is_err());
        assert!(build_archive(&[dir.clone(), "/tmp/../a.tar".into()]).is_err());
        let cmd = build_archive(&[dir, "/tmp/um-nouvelle.tar.gz".into()]).unwrap();
        assert_eq!(cmd.program, "tar");
        assert_eq!(cmd.args[0], "-czf");
        assert!(cmd.args.contains(&"--".to_string()));
    }

    #[test]
    fn flags_dangerous_archive_members() {
        assert!(unsafe_member("/etc/passwd"));
        assert!(unsafe_member("../evil"));
        assert!(unsafe_member("a/../../evil"));
        assert!(unsafe_member("~/x"));
        assert!(unsafe_member("a\x1bb"));
        assert!(!unsafe_member("dossier/fichier.txt"));
        assert!(!unsafe_member("..cache/x"));
    }

    #[test]
    fn refuses_archives_with_path_traversal() {
        let base = std::env::temp_dir().join(format!("um-evil-{}", std::process::id()));
        std::fs::create_dir_all(base.join("a")).unwrap();
        std::fs::write(base.join("a/f.txt"), "x").unwrap();
        let archive = base.join("evil.tar");
        // --absolute-names + transformation : membre "../evil.txt".
        let status = std::process::Command::new("tar")
            .args(["-cf", &archive.to_string_lossy(), "--transform=s,a/f.txt,../evil.txt,", "-C", &base.to_string_lossy(), "a/f.txt"])
            .status()
            .unwrap();
        assert!(status.success());
        let error = extract_archive(&[archive.to_string_lossy().to_string()]).unwrap_err();
        assert!(error.contains("dangereux"), "{error}");
        assert!(!base.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn extracts_a_real_archive() {
        let base = std::env::temp_dir().join(format!("um-arch-{}", std::process::id()));
        let source = base.join("contenu");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("a.txt"), "bonjour").unwrap();
        let archive = base.join("paquet.tar.gz");
        let create = build_archive(&[source.to_string_lossy().to_string(), archive.to_string_lossy().to_string()]).unwrap();
        create.run().unwrap();
        extract_archive(&[archive.to_string_lossy().to_string()]).unwrap();
        assert!(base.join("paquet/contenu/a.txt").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn btrfs_targets_must_be_known_mounts() {
        let mounts = vec!["/".to_string(), "/data".to_string()];
        assert!(btrfs_target_ok(&mounts, "/data"));
        assert!(!btrfs_target_ok(&mounts, "/etc"));
        assert!(!btrfs_target_ok(&mounts, "/data/../etc"));
        assert!(!btrfs_target_ok(&mounts, "--help"));
    }
}
