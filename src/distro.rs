//! Détection de la distribution et des outils disponibles.

use crate::exec;
use std::path::Path;

/// Contenu utile de `/etc/os-release`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OsRelease {
    pub id: String,
    pub id_like: Vec<String>,
    pub name: String,
    pub version: String,
    pub pretty_name: String,
}

impl OsRelease {
    /// Nom affichable : `PRETTY_NAME`, sinon `NAME VERSION`.
    pub fn display_name(&self) -> String {
        if !self.pretty_name.is_empty() {
            return self.pretty_name.clone();
        }
        match (self.name.is_empty(), self.version.is_empty()) {
            (false, false) => format!("{} {}", self.name, self.version),
            (false, true) => self.name.clone(),
            _ => "Linux".to_string(),
        }
    }

    /// Identifiants à comparer pour reconnaître une famille.
    pub fn identifiers(&self) -> Vec<String> {
        let mut identifiers = Vec::with_capacity(self.id_like.len() + 1);
        if !self.id.is_empty() {
            identifiers.push(self.id.clone());
        }
        identifiers.extend(self.id_like.iter().cloned());
        identifiers
    }
}

/// Analyse `/etc/os-release` sans dépendre de sa mise en forme.
pub fn parse_os_release(text: &str) -> OsRelease {
    let mut release = OsRelease::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let value = raw_value.trim().trim_matches('"').trim_matches('\'').to_string();
        match key.trim() {
            "ID" => release.id = value.to_ascii_lowercase(),
            "ID_LIKE" => {
                release.id_like = value
                    .split_whitespace()
                    .map(|item| item.to_ascii_lowercase())
                    .collect()
            }
            "NAME" => release.name = value,
            "VERSION_ID" => release.version = value,
            "PRETTY_NAME" => release.pretty_name = value,
            _ => {}
        }
    }
    release
}

/// Lit `/etc/os-release` (ou un chemin donné pour les tests).
pub fn read_os_release(path: &Path) -> OsRelease {
    std::fs::read_to_string(path)
        .map(|text| parse_os_release(&text))
        .unwrap_or_default()
}

/// Distribution du système courant.
pub fn current() -> OsRelease {
    read_os_release(Path::new("/etc/os-release"))
}

/// Noyau en cours d'exécution (`uname -r`).
pub fn kernel_release() -> String {
    exec::run("uname", &["-r"])
        .map(|captured| captured.text())
        .unwrap_or_default()
}

/// Outils optionnels détectés une fois au démarrage.
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub snap: bool,
    pub flatpak: bool,
    pub ufw: bool,
    pub firewall_cmd: bool,
    pub timeshift: bool,
    pub smartctl: bool,
    pub rsync: bool,
    pub rclone: bool,
    pub docker: bool,
    pub podman: bool,
    pub btrfs: bool,
    pub lvm: bool,
    pub ss: bool,
    pub find: bool,
    pub journalctl: bool,
    pub pkexec: bool,
    pub systemctl: bool,
    pub rpm: bool,
    pub debsums: bool,
    pub nmap: bool,
}

impl Capabilities {
    /// Sonde le système (une petite dizaine de recherches dans `PATH`).
    pub fn probe() -> Self {
        Self {
            snap: exec::exists("snap"),
            flatpak: exec::exists("flatpak"),
            ufw: exec::exists("ufw"),
            firewall_cmd: exec::exists("firewall-cmd"),
            timeshift: exec::exists("timeshift"),
            smartctl: exec::exists("smartctl"),
            rsync: exec::exists("rsync"),
            rclone: exec::exists("rclone"),
            docker: exec::exists("docker"),
            podman: exec::exists("podman"),
            btrfs: exec::exists("btrfs"),
            lvm: exec::exists("lvm"),
            ss: exec::exists("ss"),
            find: exec::exists("find"),
            journalctl: exec::exists("journalctl"),
            pkexec: exec::exists("pkexec"),
            systemctl: exec::exists("systemctl"),
            rpm: exec::exists("rpm"),
            debsums: exec::exists("debsums"),
            nmap: exec::exists("nmap"),
        }
    }

    /// Liste lisible des outils absents, pour l'écran « À propos ».
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.snap {
            missing.push("snap");
        }
        if !self.flatpak {
            missing.push("flatpak");
        }
        if !self.ufw {
            missing.push("ufw");
        }
        if !self.timeshift {
            missing.push("timeshift");
        }
        if !self.smartctl {
            missing.push("smartctl");
        }
        if !self.rsync {
            missing.push("rsync");
        }
        if !self.rclone {
            missing.push("rclone");
        }
        if !self.docker {
            missing.push("docker");
        }
        if !self.btrfs {
            missing.push("btrfs");
        }
        if !self.lvm {
            missing.push("lvm");
        }
        missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UBUNTU: &str = r#"PRETTY_NAME="Ubuntu 24.04.1 LTS"
NAME="Ubuntu"
VERSION_ID="24.04"
ID=ubuntu
ID_LIKE=debian
"#;

    const ARCH: &str = "NAME=\"Arch Linux\"\nID=arch\nPRETTY_NAME=\"Arch Linux\"\n";

    #[test]
    fn parses_ubuntu_release() {
        let release = parse_os_release(UBUNTU);
        assert_eq!(release.id, "ubuntu");
        assert_eq!(release.id_like, vec!["debian"]);
        assert_eq!(release.version, "24.04");
        assert_eq!(release.display_name(), "Ubuntu 24.04.1 LTS");
        assert!(release.identifiers().contains(&"debian".to_string()));
    }

    #[test]
    fn parses_arch_release_without_id_like() {
        let release = parse_os_release(ARCH);
        assert_eq!(release.id, "arch");
        assert!(release.id_like.is_empty());
        assert_eq!(release.display_name(), "Arch Linux");
    }

    #[test]
    fn falls_back_to_linux_when_empty() {
        let release = parse_os_release("");
        assert_eq!(release.display_name(), "Linux");
    }
}
