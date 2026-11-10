//! Audit CIS : évaluation en lecture seule du durcissement du système.

use crate::exec;
use serde::Serialize;

/// Résultat d'un contrôle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Status {
    Pass,
    Warn,
    Fail,
    Unknown,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Pass => "conforme",
            Status::Warn => "partiel",
            Status::Fail => "non conforme",
            Status::Unknown => "inconnu",
        }
    }

    fn points(&self) -> u32 {
        match self {
            Status::Pass => 2,
            Status::Warn => 1,
            Status::Fail | Status::Unknown => 0,
        }
    }
}

/// Un contrôle de durcissement.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub status: Status,
    pub detail: String,
    pub remediation: &'static str,
}

/// Faits système collectés une fois, puis évalués.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub sshd_config: String,
    pub ufw_active: bool,
    pub ufw_deny_incoming: bool,
    pub apparmor_active: bool,
    pub unattended_enabled: bool,
    pub auditd_active: bool,
    pub aslr: Option<String>,
    pub suid_dumpable: Option<String>,
    pub ip_forward: Option<String>,
    pub pass_max_days: Option<u32>,
    pub tmp_nodev: bool,
    pub tmp_nosuid: bool,
}

/// Rapport d'audit.
#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    pub checks: Vec<Check>,
    pub score: u32,
    pub categories: Vec<(String, u32)>,
    pub errors: Vec<String>,
}

impl AuditReport {
    pub fn count(&self, status: Status) -> usize {
        self.checks.iter().filter(|check| check.status == status).count()
    }
}

/// Analyse `ufw status verbose` : (actif, refus par défaut en entrée).
pub fn parse_ufw(text: &str) -> (bool, bool) {
    let lower = text.to_ascii_lowercase();
    let active = lower.lines().any(|line| line.trim() == "status: active");
    let deny_incoming = lower
        .lines()
        .any(|line| line.contains("default:") && line.contains("deny") && line.contains("incoming"));
    (active, deny_incoming)
}

/// Extrait `PASS_MAX_DAYS` de `/etc/login.defs`.
pub fn parse_pass_max_days(text: &str) -> Option<u32> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            if fields.next()? != "PASS_MAX_DAYS" {
                return None;
            }
            fields.next()?.parse().ok()
        })
}

/// Options du montage `/tmp` dans `/proc/mounts`.
pub fn parse_tmp_options(text: &str) -> (bool, bool) {
    text.lines()
        .find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 4 || fields[1] != "/tmp" {
                return None;
            }
            let options: Vec<&str> = fields[3].split(',').collect();
            Some((options.contains(&"nodev"), options.contains(&"nosuid")))
        })
        .unwrap_or((false, false))
}

fn sysctl(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

fn unit_active(unit: &str) -> bool {
    exec::run("systemctl", &["is-active", unit])
        .map(|captured| captured.text().trim() == "active")
        .unwrap_or(false)
}

fn unit_enabled(unit: &str) -> bool {
    exec::run("systemctl", &["is-enabled", unit])
        .map(|captured| captured.text().trim() == "enabled")
        .unwrap_or(false)
}

/// Collecte les faits depuis le système.
pub fn gather() -> Facts {
    let sshd_config = std::fs::read_to_string("/etc/ssh/sshd_config").unwrap_or_default();
    let ufw = exec::run("ufw", &["status", "verbose"])
        .map(|captured| parse_ufw(&captured.text()))
        .unwrap_or((false, false));
    let login_defs = std::fs::read_to_string("/etc/login.defs").unwrap_or_default();
    let (tmp_nodev, tmp_nosuid) = std::fs::read_to_string("/proc/mounts")
        .map(|text| parse_tmp_options(&text))
        .unwrap_or((false, false));
    Facts {
        sshd_config,
        ufw_active: ufw.0,
        ufw_deny_incoming: ufw.1,
        apparmor_active: unit_active("apparmor"),
        unattended_enabled: unit_enabled("unattended-upgrades"),
        auditd_active: unit_active("auditd"),
        aslr: sysctl("/proc/sys/kernel/randomize_va_space"),
        suid_dumpable: sysctl("/proc/sys/fs/suid_dumpable"),
        ip_forward: sysctl("/proc/sys/net/ipv4/ip_forward"),
        pass_max_days: parse_pass_max_days(&login_defs),
        tmp_nodev,
        tmp_nosuid,
    }
}

fn check(
    id: &'static str,
    title: &'static str,
    category: &'static str,
    status: Status,
    detail: String,
    remediation: &'static str,
) -> Check {
    Check {
        id,
        title,
        category,
        status,
        detail,
        remediation,
    }
}

/// Évalue les contrôles à partir des faits (fonction pure, testable).
pub fn evaluate(facts: &Facts) -> Vec<Check> {
    let mut checks = Vec::new();

    let root_login = crate::security::sshd_option(&facts.sshd_config, "PermitRootLogin");
    let (status, detail) = match root_login.as_deref() {
        Some("no") => (Status::Pass, "PermitRootLogin no".to_string()),
        Some(value) => (Status::Fail, format!("PermitRootLogin {value}")),
        None => (
            Status::Warn,
            "PermitRootLogin non défini (défaut dépendant de la version)".to_string(),
        ),
    };
    checks.push(check(
        "ssh-root",
        "Connexion root en SSH interdite",
        "SSH",
        status,
        detail,
        "Définir « PermitRootLogin no » dans /etc/ssh/sshd_config.",
    ));

    let password_auth = crate::security::sshd_option(&facts.sshd_config, "PasswordAuthentication");
    let (status, detail) = match password_auth.as_deref() {
        Some("no") => (Status::Pass, "PasswordAuthentication no".to_string()),
        Some(value) => (Status::Fail, format!("PasswordAuthentication {value}")),
        None => (Status::Warn, "authentification par mot de passe non définie".to_string()),
    };
    checks.push(check(
        "ssh-password",
        "Authentification SSH par clé uniquement",
        "SSH",
        status,
        detail,
        "Désactiver PasswordAuthentication après avoir déployé des clés.",
    ));

    let (status, detail) = if facts.ufw_active && facts.ufw_deny_incoming {
        (Status::Pass, "pare-feu actif, refus par défaut en entrée".to_string())
    } else if facts.ufw_active {
        (
            Status::Warn,
            "pare-feu actif mais politique d'entrée non restrictive".to_string(),
        )
    } else {
        (Status::Fail, "pare-feu inactif".to_string())
    };
    checks.push(check(
        "firewall",
        "Pare-feu actif avec refus par défaut",
        "Réseau",
        status,
        detail,
        "Activer ufw puis « ufw default deny incoming ».",
    ));

    checks.push(check(
        "apparmor",
        "AppArmor actif",
        "Noyau",
        if facts.apparmor_active {
            Status::Pass
        } else {
            Status::Fail
        },
        if facts.apparmor_active {
            "service apparmor actif".to_string()
        } else {
            "service apparmor inactif ou absent".to_string()
        },
        "Installer apparmor et activer le service.",
    ));

    checks.push(check(
        "unattended",
        "Mises à jour de sécurité automatiques",
        "Mises à jour",
        if facts.unattended_enabled {
            Status::Pass
        } else {
            Status::Fail
        },
        if facts.unattended_enabled {
            "unattended-upgrades activé".to_string()
        } else {
            "unattended-upgrades non activé".to_string()
        },
        "Activer unattended-upgrades (apt-daily-upgrade.timer).",
    ));

    let aslr_ok = facts.aslr.as_deref() == Some("2");
    checks.push(check(
        "aslr",
        "ASLR complet",
        "Noyau",
        if aslr_ok { Status::Pass } else { Status::Fail },
        format!(
            "kernel.randomize_va_space = {}",
            facts.aslr.as_deref().unwrap_or("illisible")
        ),
        "Régler kernel.randomize_va_space=2.",
    ));

    let dump_ok = facts.suid_dumpable.as_deref() == Some("0");
    checks.push(check(
        "suid-dump",
        "Core dumps des binaires SUID désactivés",
        "Noyau",
        if dump_ok { Status::Pass } else { Status::Warn },
        format!(
            "fs.suid_dumpable = {}",
            facts.suid_dumpable.as_deref().unwrap_or("illisible")
        ),
        "Régler fs.suid_dumpable=0.",
    ));

    let forward_ok = facts.ip_forward.as_deref() == Some("0");
    checks.push(check(
        "ip-forward",
        "Routage IPv4 désactivé",
        "Réseau",
        if forward_ok { Status::Pass } else { Status::Warn },
        format!(
            "net.ipv4.ip_forward = {}",
            facts.ip_forward.as_deref().unwrap_or("illisible")
        ),
        "Régler net.ipv4.ip_forward=0 sauf si la machine est un routeur.",
    ));

    let aging = facts.pass_max_days;
    let (status, detail) = match aging {
        Some(days) if days <= 365 => (Status::Pass, format!("PASS_MAX_DAYS {days}")),
        Some(days) => (Status::Warn, format!("PASS_MAX_DAYS {days} (supérieur à 365)")),
        None => (Status::Unknown, "PASS_MAX_DAYS illisible".to_string()),
    };
    checks.push(check(
        "password-aging",
        "Expiration des mots de passe",
        "Accès",
        status,
        detail,
        "Régler PASS_MAX_DAYS à 365 ou moins dans /etc/login.defs.",
    ));

    let tmp_ok = facts.tmp_nodev && facts.tmp_nosuid;
    checks.push(check(
        "tmp-options",
        "/tmp monté avec nodev et nosuid",
        "Système de fichiers",
        if tmp_ok { Status::Pass } else { Status::Warn },
        format!(
            "nodev={} nosuid={}",
            if facts.tmp_nodev { "oui" } else { "non" },
            if facts.tmp_nosuid { "oui" } else { "non" }
        ),
        "Monter /tmp avec nodev,nosuid (ou via tmpfs).",
    ));

    checks.push(check(
        "auditd",
        "Journalisation d'audit active",
        "Audit",
        if facts.auditd_active {
            Status::Pass
        } else {
            Status::Warn
        },
        if facts.auditd_active {
            "service auditd actif".to_string()
        } else {
            "auditd inactif ou non installé".to_string()
        },
        "Installer auditd et activer le service.",
    ));

    checks
}

fn percentage(checks: &[&Check]) -> u32 {
    let counted: Vec<&&Check> = checks
        .iter()
        .filter(|check| check.status != Status::Unknown)
        .collect();
    if counted.is_empty() {
        return 0;
    }
    let earned: u32 = counted.iter().map(|check| check.status.points()).sum();
    (earned * 100) / (counted.len() as u32 * 2)
}

/// Calcule le score global et par catégorie.
pub fn summarize(checks: Vec<Check>) -> AuditReport {
    let mut categories: Vec<(String, u32)> = Vec::new();
    let all_refs: Vec<&Check> = checks.iter().collect();
    let score = percentage(&all_refs);
    let mut names: Vec<&str> = checks.iter().map(|check| check.category).collect();
    names.sort_unstable();
    names.dedup();
    for name in names {
        let subset: Vec<&Check> = checks
            .iter()
            .filter(|check| check.category == name)
            .collect();
        categories.push((name.to_string(), percentage(&subset)));
    }
    AuditReport {
        checks,
        score,
        categories,
        errors: Vec::new(),
    }
}

/// Lance l'audit complet.
pub fn report() -> AuditReport {
    let facts = gather();
    let mut report = summarize(evaluate(&facts));
    if !exec::exists("ufw") {
        report.errors.push("ufw absent : contrôle pare-feu partiel".to_string());
    }
    if !exec::exists("systemctl") {
        report.errors.push("systemctl absent : services non vérifiés".to_string());
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardened() -> Facts {
        Facts {
            sshd_config: "PermitRootLogin no\nPasswordAuthentication no\n".to_string(),
            ufw_active: true,
            ufw_deny_incoming: true,
            apparmor_active: true,
            unattended_enabled: true,
            auditd_active: true,
            aslr: Some("2".to_string()),
            suid_dumpable: Some("0".to_string()),
            ip_forward: Some("0".to_string()),
            pass_max_days: Some(365),
            tmp_nodev: true,
            tmp_nosuid: true,
        }
    }

    #[test]
    fn hardened_system_scores_full() {
        let report = summarize(evaluate(&hardened()));
        assert_eq!(report.score, 100);
        assert_eq!(report.count(Status::Fail), 0);
        assert!(report.checks.len() >= 10);
    }

    #[test]
    fn lax_system_flags_failures() {
        let facts = Facts {
            sshd_config: "PermitRootLogin yes\n".to_string(),
            ufw_active: false,
            aslr: Some("0".to_string()),
            pass_max_days: Some(9999),
            ..Facts::default()
        };
        let report = summarize(evaluate(&facts));
        assert!(report.score < 40);
        let ssh = report.checks.iter().find(|check| check.id == "ssh-root").unwrap();
        assert_eq!(ssh.status, Status::Fail);
        let password = report
            .checks
            .iter()
            .find(|check| check.id == "ssh-password")
            .unwrap();
        assert_eq!(password.status, Status::Warn);
        let aging = report
            .checks
            .iter()
            .find(|check| check.id == "password-aging")
            .unwrap();
        assert_eq!(aging.status, Status::Warn);
    }

    #[test]
    fn unknown_values_are_excluded_from_score() {
        let facts = Facts {
            pass_max_days: None,
            ..hardened()
        };
        let report = summarize(evaluate(&facts));
        assert_eq!(report.score, 100);
        assert_eq!(report.count(Status::Unknown), 1);
    }

    #[test]
    fn parses_ufw_status() {
        let text = "Status: active\nLogging: on (low)\nDefault: deny (incoming), allow (outgoing), disabled (routed)\n";
        assert_eq!(parse_ufw(text), (true, true));
        assert_eq!(parse_ufw("Status: inactive\n"), (false, false));
    }

    #[test]
    fn parses_login_defs() {
        assert_eq!(parse_pass_max_days("# comment\nPASS_MAX_DAYS\t90\n"), Some(90));
        assert_eq!(parse_pass_max_days("PASS_MAX_DAYS 99999\n"), Some(99999));
        assert_eq!(parse_pass_max_days(""), None);
    }

    #[test]
    fn parses_tmp_mount_options() {
        let text = "/dev/sda1 / ext4 rw 0 0\ntmpfs /tmp tmpfs rw,nosuid,nodev 0 0\n";
        assert_eq!(parse_tmp_options(text), (true, true));
        assert_eq!(parse_tmp_options("/dev/sda1 / ext4 rw 0 0\n"), (false, false));
    }

    #[test]
    fn category_scores_are_bounded() {
        let report = summarize(evaluate(&hardened()));
        assert!(report.categories.iter().any(|(name, _)| name == "SSH"));
        assert!(report.categories.iter().all(|(_, score)| *score <= 100));
    }
}
