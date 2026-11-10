//! Sauvegarde et récupération : liste des logiciels, copie rsync avec
//! simulation, dotfiles et configuration GRUB.

use crate::exec;
use crate::inventory::Inventory;
use crate::util;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Une entrée de la liste des logiciels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareEntry {
    pub name: String,
    pub version: String,
    pub source: String,
}

/// Liste exportable des logiciels installés.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareList {
    pub generated: String,
    pub distribution: String,
    pub packages: Vec<SoftwareEntry>,
}

/// Un fichier de configuration personnelle.
#[derive(Debug, Clone, Serialize)]
pub struct Dotfile {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
}

/// Options d'une copie rsync.
#[derive(Debug, Clone)]
pub struct RsyncOptions {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub mirror: bool,
    pub delete: bool,
    pub dry_run: bool,
}

/// Configuration GRUB lue dans `/etc/default/grub`.
#[derive(Debug, Clone, Default)]
pub struct GrubConfig {
    pub default: String,
    pub timeout: String,
    pub cmdline: String,
    pub kernels: Vec<String>,
}

/// État de la page Sauvegarde.
#[derive(Debug, Clone, Default)]
pub struct BackupReport {
    pub dotfiles: Vec<Dotfile>,
    pub grub: GrubConfig,
}

/// Construit la liste des logiciels à partir de l'inventaire.
pub fn software_list(inventory: &Inventory) -> SoftwareList {
    let mut packages: Vec<SoftwareEntry> = inventory
        .packages
        .iter()
        .map(|package| SoftwareEntry {
            name: package.name.clone(),
            version: package.version.clone(),
            source: package.source.label().to_string(),
        })
        .collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    SoftwareList {
        generated: util::now_human(),
        distribution: crate::distro::current().display_name(),
        packages,
    }
}

pub fn write_list(list: &SoftwareList, path: &Path) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(list)?;
    std::fs::write(path, text)
}

pub fn read_list(path: &Path) -> Result<SoftwareList, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| format!("fichier illisible : {error}"))
}

/// Réinstalle les logiciels d'une liste, source par source, via Polkit.
pub fn install(list: &SoftwareList) -> Result<String, String> {
    let mut report = Vec::new();
    let mut failures = 0usize;

    let native: Vec<String> = list
        .packages
        .iter()
        .filter(|entry| entry.source == "Paquet")
        .map(|entry| entry.name.clone())
        .filter(|name| util::valid_package_name(name))
        .collect();
    if !native.is_empty() {
        let mut args = vec!["install".to_string(), "-y".to_string()];
        args.extend(native.iter().cloned());
        match exec::run_privileged("apt-get", &args) {
            Ok(_) => report.push(format!("APT : {} paquet(s) installé(s)", native.len())),
            Err(error) => {
                failures += 1;
                report.push(format!("APT : échec — {error}"));
            }
        }
    }

    let snap: Vec<String> = list
        .packages
        .iter()
        .filter(|entry| entry.source == "Snap")
        .map(|entry| entry.name.clone())
        .filter(|name| util::valid_package_name(name))
        .collect();
    if !snap.is_empty() {
        let mut args = vec!["install".to_string()];
        args.extend(snap.iter().cloned());
        match exec::run_privileged("snap", &args) {
            Ok(_) => report.push(format!("Snap : {} logiciel(s) installé(s)", snap.len())),
            Err(error) => {
                failures += 1;
                report.push(format!("Snap : échec — {error}"));
            }
        }
    }

    let flatpak: Vec<String> = list
        .packages
        .iter()
        .filter(|entry| entry.source == "Flatpak")
        .map(|entry| entry.name.clone())
        .filter(|name| util::valid_app_id(name))
        .collect();
    if !flatpak.is_empty() {
        let mut args = vec!["install".to_string(), "-y".to_string()];
        args.extend(flatpak.iter().cloned());
        match exec::run_privileged("flatpak", &args) {
            Ok(_) => report.push(format!("Flatpak : {} application(s) installée(s)", flatpak.len())),
            Err(error) => {
                failures += 1;
                report.push(format!("Flatpak : échec — {error}"));
            }
        }
    }

    if report.is_empty() {
        return Err("aucun logiciel à réinstaller dans ce fichier".to_string());
    }
    if failures > 0 && failures == report.len() {
        return Err(report.join("\n"));
    }
    Ok(report.join("\n"))
}

/// Valide une copie rsync avant de la lancer.
pub fn validate_rsync(options: &RsyncOptions) -> Result<(), String> {
    if !options.source.exists() {
        return Err(format!(
            "source introuvable : {}",
            options.source.display()
        ));
    }
    if options.source == Path::new("/") {
        return Err("refus de copier la racine du système".to_string());
    }
    if options.destination == Path::new("/") {
        return Err("destination refusée : racine du système".to_string());
    }
    if options.destination == options.source {
        return Err("source et destination identiques".to_string());
    }
    let source = util::normalize(&options.source);
    let destination = util::normalize(&options.destination);
    if destination.starts_with(&source) {
        return Err("la destination ne peut pas être à l'intérieur de la source".to_string());
    }
    if options.delete && !options.dry_run {
        // `--delete` ne s'applique qu'à une destination existante et distincte :
        // la garde ci-dessus protège déjà de l'effacement de la source.
        if !destination.exists() {
            return Err("--delete exige une destination existante".to_string());
        }
    }
    Ok(())
}

/// Construit les arguments rsync (aucun shell, chemins séparés).
pub fn rsync_args(options: &RsyncOptions) -> Vec<String> {
    let mut args = vec![
        "-a".to_string(),
        "--human-readable".to_string(),
        "--stats".to_string(),
    ];
    if options.mirror {
        args.push("--delete".to_string());
    } else if options.delete {
        args.push("--delete".to_string());
    }
    if options.dry_run {
        args.push("--dry-run".to_string());
    }
    // `--` : un chemin commençant par `-` ne doit jamais être lu comme une option.
    args.push("--".to_string());
    args.push(options.source.display().to_string());
    args.push(options.destination.display().to_string());
    args
}

/// Lance rsync (simulation ou copie réelle).
pub fn rsync(options: &RsyncOptions) -> Result<String, String> {
    validate_rsync(options)?;
    if !exec::exists("rsync") {
        return Err("rsync n'est pas installé".to_string());
    }
    let args = rsync_args(options);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let captured = exec::run_with_timeout(
        "rsync",
        &refs,
        std::time::Duration::from_secs(3_600),
    )
    .map_err(|error| error.0)?;
    let output = format!("{}\n{}", captured.stdout.trim(), captured.stderr.trim());
    if captured.ok {
        Ok(output)
    } else {
        Err(captured.error_text())
    }
}

/// Fichiers de configuration pris en charge (sauvegarde et restauration).
pub const DOTFILE_CANDIDATES: [&str; 13] = [
    ".bashrc",
    ".zshrc",
    ".profile",
    ".bash_aliases",
    ".gitconfig",
    ".vimrc",
    ".nanorc",
    ".tmux.conf",
    ".config/nvim",
    ".config/htop",
    ".config/kitty",
    ".config/fish",
    ".config/alacritty",
];

/// Fichiers de configuration usuels, hors secrets (`~/.ssh` ignoré).
pub fn dotfiles() -> Vec<Dotfile> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let candidates = DOTFILE_CANDIDATES;
    let mut found = Vec::new();
    for candidate in candidates {
        let path = home.join(candidate);
        if !path.exists() {
            continue;
        }
        let size = if path.is_dir() {
            util::dir_size(&path, 50_000)
        } else {
            std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0)
        };
        found.push(Dotfile {
            name: candidate.to_string(),
            path,
            size,
        });
    }
    found
}

fn copy_recursive(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)?.flatten() {
            let path = entry.path();
            let target = destination.join(entry.file_name());
            if path.is_dir() {
                copy_recursive(&path, &target)?;
            } else if path.is_file() {
                std::fs::copy(&path, &target)?;
            }
        }
        Ok(())
    } else {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, destination).map(|_| ())
    }
}

/// Copie les dotfiles détectés dans un dossier daté.
pub fn backup_dotfiles(target: &Path) -> Result<String, String> {
    let items = dotfiles();
    if items.is_empty() {
        return Err("aucun fichier de configuration détecté".to_string());
    }
    std::fs::create_dir_all(target).map_err(|error| error.to_string())?;
    let mut copied = 0usize;
    let mut errors = Vec::new();
    for item in &items {
        let destination = target.join(&item.name);
        match copy_recursive(&item.path, &destination) {
            Ok(()) => copied += 1,
            Err(error) => errors.push(format!("{} : {error}", item.name)),
        }
    }
    let mut message = format!("{copied} élément(s) sauvegardé(s) dans {}", target.display());
    if !errors.is_empty() {
        message.push_str(&format!("\néchecs : {}", errors.join(", ")));
    }
    Ok(message)
}

/// Analyse `/etc/default/grub`.
pub fn parse_grub(text: &str) -> (String, String, String) {
    let mut default = String::new();
    let mut timeout = String::new();
    let mut cmdline = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        match key.trim() {
            "GRUB_DEFAULT" => default = value,
            "GRUB_TIMEOUT" => timeout = value,
            "GRUB_CMDLINE_LINUX_DEFAULT" | "GRUB_CMDLINE_LINUX" if cmdline.is_empty() => {
                cmdline = value
            }
            _ => {}
        }
    }
    (default, timeout, cmdline)
}

/// Noyaux présents dans `/boot`.
pub fn kernels() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/boot") else {
        return Vec::new();
    };
    let mut versions: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .strip_prefix("vmlinuz-")
                .map(str::to_string)
        })
        .collect();
    versions.sort();
    versions.dedup();
    versions
}

/// Lit la configuration GRUB et les noyaux installés.
pub fn grub_config() -> GrubConfig {
    let text = std::fs::read_to_string("/etc/default/grub").unwrap_or_default();
    let (default, timeout, cmdline) = parse_grub(&text);
    GrubConfig {
        default,
        timeout,
        cmdline,
        kernels: kernels(),
    }
}

/// Régénère la configuration GRUB (Polkit).
pub fn regenerate_grub() -> Result<String, String> {
    let program = if exec::exists("update-grub") {
        "update-grub"
    } else if exec::exists("grub-mkconfig") {
        "grub-mkconfig"
    } else {
        return Err("ni update-grub ni grub-mkconfig ne sont disponibles".to_string());
    };
    let args: Vec<String> = if program == "grub-mkconfig" {
        vec!["-o".to_string(), "/boot/grub/grub.cfg".to_string()]
    } else {
        Vec::new()
    };
    exec::run_privileged(program, &args)
}

/// État initial de la page Sauvegarde.
pub fn report() -> BackupReport {
    BackupReport {
        dotfiles: dotfiles(),
        grub: grub_config(),
    }
}

/// Copie récursive qui ignore les liens symboliques (restauration depuis un
/// dossier qui n'est pas forcément de confiance).
fn copy_tree_nolinks(source: &Path, destination: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_dir() {
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)?.flatten() {
            copy_tree_nolinks(&entry.path(), &destination.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, destination).map(|_| ())
    }
}

/// Restaure les dotfiles d'un dossier de sauvegarde dans `home`. Un fichier
/// existant est d'abord renommé en `<nom>.um-bak-<horodatage>`.
pub fn restore_dotfiles_to(source: &Path, home: &Path) -> Result<String, String> {
    let meta = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if !meta.is_dir() {
        return Err("la sauvegarde doit être un dossier".to_string());
    }
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    let mut restored = 0usize;
    let mut errors = Vec::new();
    for name in DOTFILE_CANDIDATES {
        let from = source.join(name);
        match std::fs::symlink_metadata(&from) {
            Ok(meta) if !meta.file_type().is_symlink() => {}
            _ => continue,
        }
        let to = home.join(name);
        if std::fs::symlink_metadata(&to).is_ok() {
            let saved = home.join(format!("{name}.um-bak-{stamp}"));
            if let Err(error) = std::fs::rename(&to, &saved) {
                errors.push(format!("{name} : {error}"));
                continue;
            }
        }
        match copy_tree_nolinks(&from, &to) {
            Ok(()) => restored += 1,
            Err(error) => errors.push(format!("{name} : {error}")),
        }
    }
    if restored == 0 && errors.is_empty() {
        return Err("aucun fichier de configuration reconnu dans ce dossier".to_string());
    }
    let mut message = format!("{restored} élément(s) restauré(s) (les originaux sont conservés en .um-bak-{stamp})");
    if !errors.is_empty() {
        message.push_str(&format!("\néchecs : {}", errors.join(", ")));
    }
    Ok(message)
}

pub fn restore_dotfiles(source: &Path) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("dossier personnel introuvable")?;
    restore_dotfiles_to(source, &home)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> RsyncOptions {
        RsyncOptions {
            source: PathBuf::from("/home"),
            destination: PathBuf::from("/tmp/sauvegarde"),
            mirror: false,
            delete: false,
            dry_run: true,
        }
    }

    #[test]
    fn rsync_args_include_dry_run() {
        let args = rsync_args(&options());
        assert!(args.contains(&"--dry-run".to_string()));
        assert!(args.contains(&"-a".to_string()));
        assert_eq!(args.last().unwrap(), "/tmp/sauvegarde");
    }

    #[test]
    fn rsync_rejects_root_source() {
        let mut opts = options();
        opts.source = PathBuf::from("/");
        assert!(validate_rsync(&opts).is_err());
    }

    #[test]
    fn rsync_rejects_destination_inside_source() {
        let mut opts = options();
        opts.source = PathBuf::from("/home/amine");
        opts.destination = PathBuf::from("/home/amine/copie");
        assert!(validate_rsync(&opts).is_err());
    }

    #[test]
    fn rsync_rejects_missing_source() {
        let mut opts = options();
        opts.source = PathBuf::from("/chemin/qui/nexiste/pas");
        assert!(validate_rsync(&opts).is_err());
    }

    #[test]
    fn parses_grub_defaults() {
        let text = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\n# commentaire\nGRUB_CMDLINE_LINUX=\"\"\n";
        let (default, timeout, cmdline) = parse_grub(text);
        assert_eq!(default, "0");
        assert_eq!(timeout, "5");
        assert_eq!(cmdline, "quiet splash");
    }

    #[test]
    fn software_list_roundtrip() {
        let list = SoftwareList {
            generated: "2026-11-02".to_string(),
            distribution: "Ubuntu".to_string(),
            packages: vec![SoftwareEntry {
                name: "firefox".to_string(),
                version: "1.0".to_string(),
                source: "Paquet".to_string(),
            }],
        };
        let path = std::env::temp_dir().join(format!("um-list-{}.json", std::process::id()));
        write_list(&list, &path).expect("écriture");
        let read = read_list(&path).expect("lecture");
        assert_eq!(read.packages.len(), 1);
        assert_eq!(read.packages[0].name, "firefox");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dotfiles_exclude_ssh_keys() {
        let names: Vec<String> = dotfiles().into_iter().map(|file| file.name).collect();
        assert!(names.iter().all(|name| !name.contains(".ssh")));
    }

    #[test]
    fn backup_copies_files_and_directories() {
        let base = std::env::temp_dir().join(format!("um-dot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("source/sous")).unwrap();
        std::fs::write(base.join("source/a.txt"), b"a").unwrap();
        std::fs::write(base.join("source/sous/b.txt"), b"b").unwrap();
        copy_recursive(&base.join("source"), &base.join("copie")).unwrap();
        assert!(base.join("copie/a.txt").is_file());
        assert!(base.join("copie/sous/b.txt").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn restores_dotfiles_and_keeps_originals() {
        let base = std::env::temp_dir().join(format!("um-restore-{}", std::process::id()));
        let (source, home) = (base.join("backup"), base.join("home"));
        std::fs::create_dir_all(source.join(".config/nvim")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(source.join(".bashrc"), "nouveau").unwrap();
        std::fs::write(source.join(".config/nvim/init.lua"), "lua").unwrap();
        std::fs::write(source.join("autre.txt"), "ignoré").unwrap();
        std::fs::write(home.join(".bashrc"), "ancien").unwrap();
        // Un lien symbolique dans la sauvegarde n'est jamais suivi.
        std::fs::write(base.join("secret"), "secret").unwrap();
        std::os::unix::fs::symlink(base.join("secret"), source.join(".vimrc")).unwrap();

        let message = restore_dotfiles_to(&source, &home).unwrap();
        assert!(message.contains("2 élément(s)"));
        assert_eq!(std::fs::read_to_string(home.join(".bashrc")).unwrap(), "nouveau");
        assert_eq!(std::fs::read_to_string(home.join(".config/nvim/init.lua")).unwrap(), "lua");
        assert!(!home.join(".vimrc").exists());
        assert!(!home.join("autre.txt").exists());
        let kept = std::fs::read_dir(&home).unwrap().flatten().any(|entry| {
            entry.file_name().to_string_lossy().starts_with(".bashrc.um-bak-")
                && std::fs::read_to_string(entry.path()).unwrap() == "ancien"
        });
        assert!(kept, "l'ancien .bashrc doit être conservé");
        assert!(restore_dotfiles_to(&base.join("absent"), &home).is_err());
        assert!(restore_dotfiles_to(&source.join(".bashrc"), &home).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
