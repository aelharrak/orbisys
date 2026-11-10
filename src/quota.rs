//! Quotas disque : activation dans `fstab`, quotas utilisateurs et groupes,
//! seuils d'alerte et modification via `setquota`.

use crate::exec;
use crate::util;
use serde::Serialize;

/// Un montage déclarant des quotas dans `/etc/fstab`.
#[derive(Debug, Clone, Serialize)]
pub struct MountQuota {
    pub mount: String,
    pub device: String,
    pub user_quota: bool,
    pub group_quota: bool,
    pub project_quota: bool,
}

impl MountQuota {
    pub fn enabled(&self) -> bool {
        self.user_quota || self.group_quota || self.project_quota
    }
}

/// Une ligne de `repquota`.
#[derive(Debug, Clone, Serialize)]
pub struct QuotaEntry {
    pub name: String,
    pub kind: &'static str,
    pub mount: String,
    pub used: u64,
    pub soft: u64,
    pub hard: u64,
    pub files_used: u64,
    pub files_soft: u64,
    pub files_hard: u64,
    pub exceeded: bool,
}

impl QuotaEntry {
    /// Pourcentage de consommation rapporté à la limite dure, sinon souple.
    pub fn usage_percent(&self) -> f64 {
        let limit = if self.hard > 0 {
            self.hard
        } else {
            self.soft
        };
        if limit == 0 {
            0.0
        } else {
            (self.used as f64 / limit as f64) * 100.0
        }
    }

    pub fn alert(&self) -> bool {
        self.usage_percent() >= 80.0
    }
}

/// Rapport complet des quotas.
#[derive(Debug, Clone, Default)]
pub struct QuotaReport {
    pub tool_available: bool,
    pub mounts: Vec<MountQuota>,
    pub entries: Vec<QuotaEntry>,
    pub errors: Vec<String>,
}

/// Nom d'utilisateur Linux acceptable.
pub fn valid_username(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    name.len() <= 32
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Convertit `1.2G`, `512K`, `20M` en octets.
pub fn parse_size(text: &str) -> u64 {
    let text = text.trim();
    if text.is_empty() {
        return 0;
    }
    let (number, multiplier) = match text.chars().last().unwrap_or('0') {
        'K' | 'k' => (&text[..text.len() - 1], 1024u64),
        'M' | 'm' => (&text[..text.len() - 1], 1024 * 1024),
        'G' | 'g' => (&text[..text.len() - 1], 1024 * 1024 * 1024),
        'T' | 't' => (&text[..text.len() - 1], 1024u64.pow(4)),
        _ => (text, 1),
    };
    let number: f64 = number.trim().parse().unwrap_or(0.0);
    (number * multiplier as f64) as u64
}

/// Analyse `/etc/fstab` et repère les options de quota.
pub fn parse_fstab(text: &str) -> Vec<MountQuota> {
    let mut mounts: Vec<MountQuota> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        let device = fields[0].to_string();
        let mount = fields[1].to_string();
        let options: Vec<&str> = fields[3].split(',').collect();
        let user_quota = options.contains(&"usrquota") || options.contains(&"uquota");
        let group_quota = options.contains(&"grpquota") || options.contains(&"gquota");
        let project_quota = options.contains(&"prjquota");
        if user_quota || group_quota || project_quota {
            mounts.push(MountQuota {
                mount,
                device,
                user_quota,
                group_quota,
                project_quota,
            });
        }
    }
    mounts
}

/// Analyse `repquota -a -s` (unités humaines).
pub fn parse_repquota(text: &str) -> Vec<QuotaEntry> {
    let mut entries: Vec<QuotaEntry> = Vec::new();
    let mut mount = String::new();
    let mut kind: &'static str = "user";
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("*** Report for") {
            mount = trimmed
                .split(" on device ")
                .nth(1)
                .unwrap_or("")
                .trim()
                .to_string();
            kind = if trimmed.contains("group quotas") {
                "group"
            } else if trimmed.contains("project quotas") {
                "project"
            } else {
                "user"
            };
            continue;
        }
        if trimmed.starts_with("Block grace")
            || trimmed.starts_with("User ")
            || trimmed.starts_with("Group ")
            || trimmed.starts_with("Project ")
            || trimmed.starts_with('#')
        {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        // `nom indicateurs used soft hard [grâce] inodes...`
        let name = fields[0];
        let mut index = 1;
        let mut flags = "";
        if fields[index].len() <= 3 && !fields[index].chars().next().unwrap_or('0').is_ascii_digit()
        {
            flags = fields[index];
            index += 1;
        }
        // Une ligne de données commence toujours par une taille numérique :
        // cela écarte les en-têtes (« Block limits File limits »).
        if !fields
            .get(index)
            .and_then(|value| value.chars().next())
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            continue;
        }
        let used = parse_size(fields.get(index).copied().unwrap_or("0"));
        let soft = parse_size(fields.get(index + 1).copied().unwrap_or("0"));
        let hard = parse_size(fields.get(index + 2).copied().unwrap_or("0"));
        index += 3;
        // La colonne de grâce peut être présente : on la saute si elle n'est pas numérique.
        if let Some(value) = fields.get(index) {
            if value.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
                index += 1;
            }
        }
        let files_used: u64 = fields.get(index).and_then(|value| value.parse().ok()).unwrap_or(0);
        let files_soft: u64 = fields
            .get(index + 1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let files_hard: u64 = fields
            .get(index + 2)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        if !valid_username(name) {
            continue;
        }
        entries.push(QuotaEntry {
            name: name.to_string(),
            kind,
            mount: mount.clone(),
            used,
            soft,
            hard,
            files_used,
            files_soft,
            files_hard,
            exceeded: flags.contains('+') || (hard > 0 && used >= hard),
        });
    }
    entries
}

/// Vérifie les paramètres avant `setquota`.
pub fn validate_setquota(user: &str, mount: &str, soft: u64, hard: u64) -> Result<(), String> {
    if !valid_username(user) {
        return Err(format!("utilisateur refusé : {user}"));
    }
    if !mount.starts_with('/') || mount.contains("..") || mount.contains(' ') {
        return Err(format!("point de montage refusé : {mount}"));
    }
    if soft > hard {
        return Err("la limite souple dépasse la limite dure".to_string());
    }
    if hard > 1024 * 1024 * 1024 * 1024 {
        return Err("limite hors bornes (1 Tio maximum)".to_string());
    }
    Ok(())
}

/// Applique un quota utilisateur (en octets) sur un montage.
pub fn set_quota(user: &str, mount: &str, soft: u64, hard: u64) -> Result<String, String> {
    validate_setquota(user, mount, soft, hard)?;
    let to_kib = |bytes: u64| (bytes / 1024).to_string();
    exec::run_privileged(
        "setquota",
        &[
            "-u".to_string(),
            user.to_string(),
            to_kib(soft),
            to_kib(hard),
            "0".to_string(),
            "0".to_string(),
            mount.to_string(),
        ],
    )?;
    Ok(format!(
        "quota appliqué : {user} sur {mount} ({} / {})",
        util::human_bytes(soft),
        util::human_bytes(hard)
    ))
}

/// Active les quotas sur un montage (`quotaon`).
pub fn enable(mount: &str) -> Result<String, String> {
    if !mount.starts_with('/') {
        return Err(format!("point de montage refusé : {mount}"));
    }
    exec::run_privileged("quotaon", &[mount.to_string()])?;
    Ok(format!("quotas activés : {mount}"))
}

/// Collecte l'état des quotas.
pub fn report() -> QuotaReport {
    let mut report = QuotaReport {
        tool_available: exec::exists("repquota"),
        ..QuotaReport::default()
    };
    if let Ok(text) = std::fs::read_to_string("/etc/fstab") {
        report.mounts = parse_fstab(&text);
    }
    if !report.tool_available {
        report
            .errors
            .push("repquota absent : paquet quota non installé".to_string());
        return report;
    }
    match exec::run_with_timeout(
        "repquota",
        &["-a", "-s"],
        std::time::Duration::from_secs(45),
    ) {
        Ok(captured) => {
            report.entries = parse_repquota(&captured.stdout);
            if report.entries.is_empty() && !captured.ok {
                report.errors.push(captured.error_text());
            }
        }
        Err(error) => report.errors.push(error.0),
    }
    report.entries.sort_by(|a, b| {
        b.usage_percent()
            .partial_cmp(&a.usage_percent())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_human_sizes() {
        assert_eq!(parse_size("0"), 0);
        assert_eq!(parse_size("512K"), 512 * 1024);
        assert_eq!(parse_size("1.5M"), 1024 * 1024 + 512 * 1024);
        assert_eq!(parse_size("2G"), 2 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("1T"), 1024u64.pow(4));
    }

    #[test]
    fn detects_fstab_quota_options() {
        let text = "UUID=abc / ext4 defaults,usrquota,grpquota 0 1\n/dev/sdb1 /home ext4 defaults 0 2\n/dev/sdc1 /srv xfs defaults,prjquota 0 0\n";
        let mounts = parse_fstab(text);
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].mount, "/");
        assert!(mounts[0].user_quota && mounts[0].group_quota);
        assert!(mounts[1].project_quota);
        assert!(mounts[0].enabled());
    }

    #[test]
    fn parses_repquota_output() {
        let text = "*** Report for user quotas on device /dev/sda2\nBlock grace time: 7days; Inode grace time: 7days\n                        Block limits                File limits\nUser            used    soft    hard  grace    used  soft  hard  grace\n----------------------------------------------------------------------\nroot      --     20K      0K      0K             12     0     0\namine     --   2.4G   2.0G   2.5G           1234     0     0\n";
        let entries = parse_repquota(text);
        assert_eq!(entries.len(), 2);
        let amine = entries.iter().find(|entry| entry.name == "amine").unwrap();
        assert_eq!(amine.kind, "user");
        assert_eq!(amine.mount, "/dev/sda2");
        assert_eq!(amine.used, parse_size("2.4G"));
        assert!(amine.alert());
        assert!(!amine.exceeded);
    }

    #[test]
    fn flags_exceeded_entries() {
        let text = "*** Report for group quotas on device /dev/sdb1\nGroup            used    soft    hard  grace    used  soft  hard\nstaff     +-     3G      2G      2G           10     0     0\n";
        let entries = parse_repquota(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "group");
        assert!(entries[0].exceeded);
    }

    #[test]
    fn validates_quota_requests() {
        assert!(validate_setquota("amine", "/home", 1024, 2048).is_ok());
        assert!(validate_setquota("amine", "/home", 4096, 2048).is_err());
        assert!(validate_setquota("root; reboot", "/home", 1, 2).is_err());
        assert!(valid_username("root"));
        assert!(valid_username("_service"));
        assert!(!valid_username("1root"));
        assert!(!valid_username("a b"));
        assert!(validate_setquota("amine", "home", 1, 2).is_err());
        assert!(validate_setquota("amine", "/../etc", 1, 2).is_err());
    }
}
