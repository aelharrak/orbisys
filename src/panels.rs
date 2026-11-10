//! Catalogue des rubriques décrites par le modèle générique `panel`.

use crate::panel::Panel;
use crate::{p_scan, p_backup, p_hardware, p_monitor, p_network, p_security, p_software, p_storage, p_system};

/// Identifiant d'une rubrique générique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    Monitor,
    Alerts,
    Health,
    History,
    Delivery,
    Benchmark,
    AptRepos,
    AdvPackages,
    Runtimes,
    Containers,
    ContainersAdv,
    Startup,
    ActionLog,
    Maintenance,
    Troubleshoot,
    Profiles,
    Users,
    AdvCleanup,
    Mounts,
    UsbBoot,
    Archives,
    Traffic,
    Lan,
    SshGpg,
    Certs,
    SecureBoot,
    AppArmor,
    Integrity,
    Acl,
    Bluetooth,
    Power,
    Fans,
    CloudBackup,
    Schedule,
    WifiVpn,
    Firmware,
    BtrfsMaint,
    Dotfiles,
    Scan,
}

impl PanelId {
    pub const ALL: &'static [PanelId] = &[
        PanelId::Monitor,
        PanelId::Alerts,
        PanelId::Health,
        PanelId::History,
        PanelId::Delivery,
        PanelId::Benchmark,
        PanelId::AptRepos,
        PanelId::AdvPackages,
        PanelId::Runtimes,
        PanelId::Containers,
        PanelId::ContainersAdv,
        PanelId::Startup,
        PanelId::ActionLog,
        PanelId::Maintenance,
        PanelId::Troubleshoot,
        PanelId::Profiles,
        PanelId::Users,
        PanelId::AdvCleanup,
        PanelId::Mounts,
        PanelId::UsbBoot,
        PanelId::Archives,
        PanelId::Traffic,
        PanelId::Lan,
        PanelId::SshGpg,
        PanelId::Certs,
        PanelId::SecureBoot,
        PanelId::AppArmor,
        PanelId::Integrity,
        PanelId::Acl,
        PanelId::Bluetooth,
        PanelId::Power,
        PanelId::Fans,
        PanelId::CloudBackup,
        PanelId::Schedule,
        PanelId::WifiVpn,
        PanelId::Firmware,
        PanelId::BtrfsMaint,
        PanelId::Dotfiles,
        PanelId::Scan,
    ];

    /// Clé de traduction du titre.
    pub fn key(self) -> &'static str {
        match self {
            PanelId::Monitor => "page.monitor",
            PanelId::Alerts => "page.alerts",
            PanelId::Health => "page.health",
            PanelId::History => "page.history",
            PanelId::Delivery => "page.delivery",
            PanelId::Benchmark => "page.benchmark",
            PanelId::AptRepos => "page.aptrepos",
            PanelId::AdvPackages => "page.advpackages",
            PanelId::Runtimes => "page.runtimes",
            PanelId::Containers => "page.containers",
            PanelId::ContainersAdv => "page.containersadv",
            PanelId::Startup => "page.startup",
            PanelId::ActionLog => "page.actionlog",
            PanelId::Maintenance => "page.maintenance",
            PanelId::Troubleshoot => "page.troubleshoot",
            PanelId::Profiles => "page.profiles",
            PanelId::Users => "page.users",
            PanelId::AdvCleanup => "page.advcleanup",
            PanelId::Mounts => "page.mounts",
            PanelId::UsbBoot => "page.usbboot",
            PanelId::Archives => "page.archives",
            PanelId::Traffic => "page.traffic",
            PanelId::Lan => "page.lan",
            PanelId::SshGpg => "page.sshgpg",
            PanelId::Certs => "page.certs",
            PanelId::SecureBoot => "page.secureboot",
            PanelId::AppArmor => "page.apparmor",
            PanelId::Integrity => "page.integrity",
            PanelId::Acl => "page.acl",
            PanelId::Bluetooth => "page.bluetooth",
            PanelId::Power => "page.power",
            PanelId::Fans => "page.fans",
            PanelId::CloudBackup => "page.cloudbackup",
            PanelId::Schedule => "page.schedule",
            PanelId::WifiVpn => "page.wifivpn",
            PanelId::Firmware => "page.firmware",
            PanelId::BtrfsMaint => "page.btrfsmaint",
            PanelId::Dotfiles => "page.dotfiles",
            PanelId::Scan => "page.scan",
        }
    }

    /// Nom court (`monitor`, `lan`…) utilisé par la ligne de commande.
    pub fn name(self) -> &'static str {
        self.key().trim_start_matches("page.")
    }

    pub fn from_name(name: &str) -> Option<PanelId> {
        PanelId::ALL.iter().copied().find(|id| id.name() == name)
    }

    /// Clé de traduction du conseil.
    pub fn hint_key(self) -> &'static str {
        match self {
            PanelId::Monitor => "hint.monitor",
            PanelId::Alerts => "hint.alerts",
            PanelId::Health => "hint.health",
            PanelId::History => "hint.history",
            PanelId::Delivery => "hint.delivery",
            PanelId::Benchmark => "hint.benchmark",
            PanelId::AptRepos => "hint.aptrepos",
            PanelId::AdvPackages => "hint.advpackages",
            PanelId::Runtimes => "hint.runtimes",
            PanelId::Containers => "hint.containers",
            PanelId::ContainersAdv => "hint.containersadv",
            PanelId::Startup => "hint.startup",
            PanelId::ActionLog => "hint.actionlog",
            PanelId::Maintenance => "hint.maintenance",
            PanelId::Troubleshoot => "hint.troubleshoot",
            PanelId::Profiles => "hint.profiles",
            PanelId::Users => "hint.users",
            PanelId::AdvCleanup => "hint.advcleanup",
            PanelId::Mounts => "hint.mounts",
            PanelId::UsbBoot => "hint.usbboot",
            PanelId::Archives => "hint.archives",
            PanelId::Traffic => "hint.traffic",
            PanelId::Lan => "hint.lan",
            PanelId::SshGpg => "hint.sshgpg",
            PanelId::Certs => "hint.certs",
            PanelId::SecureBoot => "hint.secureboot",
            PanelId::AppArmor => "hint.apparmor",
            PanelId::Integrity => "hint.integrity",
            PanelId::Acl => "hint.acl",
            PanelId::Bluetooth => "hint.bluetooth",
            PanelId::Power => "hint.power",
            PanelId::Fans => "hint.fans",
            PanelId::CloudBackup => "hint.cloudbackup",
            PanelId::Schedule => "hint.schedule",
            PanelId::WifiVpn => "hint.wifivpn",
            PanelId::Firmware => "hint.firmware",
            PanelId::BtrfsMaint => "hint.btrfsmaint",
            PanelId::Dotfiles => "hint.dotfiles",
            PanelId::Scan => "hint.scan",
        }
    }

    /// Rafraîchissement automatique (secondes) des écrans « en direct ».
    pub fn auto_refresh(self) -> Option<u64> {
        match self {
            PanelId::Monitor => Some(3),
            PanelId::Traffic => Some(5),
            _ => None,
        }
    }

    /// Construit le contenu (peut bloquer : à lancer en tâche de fond).
    pub fn collect(self) -> Panel {
        match self {
            PanelId::Monitor => p_monitor::monitor(),
            PanelId::Alerts => p_monitor::alerts(),
            PanelId::Health => p_monitor::health(),
            PanelId::History => p_monitor::history(),
            PanelId::Delivery => p_monitor::delivery(),
            PanelId::Benchmark => p_monitor::benchmark(),
            PanelId::AptRepos => p_software::apt_repos(),
            PanelId::AdvPackages => p_software::adv_packages(),
            PanelId::Runtimes => p_software::runtimes(),
            PanelId::Containers => p_software::containers(),
            PanelId::ContainersAdv => p_software::containers_advanced(),
            PanelId::Startup => p_system::startup(),
            PanelId::ActionLog => p_system::action_log(),
            PanelId::Maintenance => p_system::maintenance(),
            PanelId::Troubleshoot => p_system::troubleshoot(),
            PanelId::Profiles => p_system::profiles(),
            PanelId::Users => p_system::users(),
            PanelId::AdvCleanup => p_storage::advanced_cleanup(),
            PanelId::Mounts => p_storage::mounts_panel(),
            PanelId::UsbBoot => p_storage::usb_bootable(),
            PanelId::Archives => p_storage::archives(),
            PanelId::Traffic => p_network::traffic(),
            PanelId::Lan => p_network::lan_discovery(),
            PanelId::SshGpg => p_security::ssh_gpg(),
            PanelId::Certs => p_security::certificates(),
            PanelId::SecureBoot => p_security::secure_boot(),
            PanelId::AppArmor => p_security::apparmor(),
            PanelId::Integrity => p_security::integrity(),
            PanelId::Acl => p_security::acl(),
            PanelId::Bluetooth => p_hardware::bluetooth_usb(),
            PanelId::Power => p_hardware::power(),
            PanelId::Fans => p_hardware::fans_sensors(),
            PanelId::CloudBackup => p_backup::cloud_backup(),
            PanelId::Schedule => p_monitor::report_schedule(),
            PanelId::WifiVpn => p_network::wifi_vpn(),
            PanelId::Firmware => p_hardware::firmware(),
            PanelId::BtrfsMaint => p_storage::btrfs_maintenance(),
            PanelId::Dotfiles => p_backup::dotfiles(),
            PanelId::Scan => p_scan::behavior_scan(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_translated() {
        let mut seen = std::collections::HashSet::new();
        for id in PanelId::ALL {
            assert!(seen.insert(id.key()), "clé en double : {}", id.key());
            for lang in [crate::i18n::Lang::Fr, crate::i18n::Lang::En, crate::i18n::Lang::Es] {
                assert_ne!(crate::i18n::t(lang, id.key()), id.key(), "titre manquant : {}", id.key());
                assert_ne!(crate::i18n::t(lang, id.hint_key()), id.hint_key(), "conseil manquant : {}", id.hint_key());
            }
        }
    }
}
