//! Audit de sécurité en lecture seule : SSH, pare-feu, mises à jour
//! automatiques, SUID, ports en écoute, Secure Boot, AppArmor.

use crate::distro::Capabilities;
use crate::exec;
use std::path::Path;

/// Gravité d'une constatation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Info,
    Warning,
    Critical,
}

impl Level {
    pub fn label(&self) -> &'static str {
        match self {
            Level::Ok => "OK",
            Level::Info => "Info",
            Level::Warning => "Attention",
            Level::Critical => "Critique",
        }
    }
}

/// Une ligne d'audit.
#[derive(Debug, Clone)]
pub struct Finding {
    pub level: Level,
    pub title: String,
    pub detail: String,
}

/// Rapport d'audit complet.
#[derive(Debug, Clone, Default)]
pub struct Audit {
    pub findings: Vec<Finding>,
    pub suid_count: usize,
    pub listening_ports: usize,
}

impl Audit {
    pub fn score(&self) -> u32 {
        let penalties: u32 = self
            .findings
            .iter()
            .map(|finding| match finding.level {
                Level::Critical => 22,
                Level::Warning => 10,
                Level::Info | Level::Ok => 0,
            })
            .sum();
        100u32.saturating_sub(penalties.min(100))
    }

    pub fn problems(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| matches!(finding.level, Level::Warning | Level::Critical))
            .count()
    }
}

/// Lit une option de `sshd_config` (dernière occurrence gagnante).
pub fn sshd_option(text: &str, key: &str) -> Option<String> {
    let mut value = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        if let (Some(name), Some(rest)) = (parts.next(), parts.next()) {
            if name.eq_ignore_ascii_case(key) {
                value = Some(rest.to_string());
            }
        }
    }
    value
}

/// `SecureBoot` depuis les variables EFI (`Some(false)` si désactivé).
pub fn secure_boot_state() -> Option<bool> {
    let dir = std::fs::read_dir("/sys/firmware/efi/efivars").ok()?;
    for entry in dir.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("SecureBoot-")
        {
            let data = std::fs::read(entry.path()).ok()?;
            let value = data.last().copied().unwrap_or(0);
            return Some(value == 1);
        }
    }
    None
}

fn firewall_active(caps: &Capabilities) -> Option<bool> {
    if caps.ufw {
        if let Ok(captured) = exec::run("systemctl", &["is-active", "ufw"]) {
            return Some(captured.text() == "active");
        }
    }
    if caps.firewall_cmd {
        if let Ok(captured) = exec::run("firewall-cmd", &["--state"]) {
            return Some(captured.text() == "running");
        }
    }
    None
}

fn suid_count(caps: &Capabilities) -> usize {
    if !caps.find {
        return 0;
    }
    let captured = exec::run_with_timeout(
        "find",
        &[
            "/usr/bin",
            "/usr/sbin",
            "/usr/local/bin",
            "-xdev",
            "-perm",
            "-4000",
            "-type",
            "f",
        ],
        std::time::Duration::from_secs(20),
    );
    match captured {
        Ok(captured) => captured
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
        Err(_) => 0,
    }
}

fn listening_ports(caps: &Capabilities) -> usize {
    if !caps.ss {
        return 0;
    }
    exec::run("ss", &["-tulpnH"])
        .map(|captured| {
            captured
                .stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

/// Réalise l'audit (aucune modification du système).
pub fn audit(capabilities: &Capabilities) -> Audit {
    let mut audit = Audit::default();
    let sshd_path = Path::new("/etc/ssh/sshd_config");

    match std::fs::read_to_string(sshd_path) {
        Ok(text) => {
            let root_login = sshd_option(&text, "PermitRootLogin").unwrap_or_else(|| "non défini".into());
            let password_auth =
                sshd_option(&text, "PasswordAuthentication").unwrap_or_else(|| "non défini".into());
            let root_ok = matches!(root_login.as_str(), "no" | "prohibit-password" | "forced-commands-only");
            audit.findings.push(Finding {
                level: if root_ok { Level::Ok } else { Level::Critical },
                title: "Connexion SSH de root".to_string(),
                detail: format!("PermitRootLogin {root_login}"),
            });
            audit.findings.push(Finding {
                level: if password_auth.eq_ignore_ascii_case("yes") {
                    Level::Warning
                } else {
                    Level::Ok
                },
                title: "Authentification par mot de passe SSH".to_string(),
                detail: format!("PasswordAuthentication {password_auth}"),
            });
        }
        Err(_) => audit.findings.push(Finding {
            level: Level::Info,
            title: "Serveur SSH absent".to_string(),
            detail: "Aucun /etc/ssh/sshd_config : rien à vérifier.".to_string(),
        }),
    }

    match firewall_active(capabilities) {
        Some(true) => audit.findings.push(Finding {
            level: Level::Ok,
            title: "Pare-feu actif".to_string(),
            detail: "Le pare-feu système est démarré.".to_string(),
        }),
        Some(false) => audit.findings.push(Finding {
            level: Level::Warning,
            title: "Pare-feu inactif".to_string(),
            detail: "Aucune règle de filtrage n'est appliquée au démarrage.".to_string(),
        }),
        None => audit.findings.push(Finding {
            level: Level::Info,
            title: "Pare-feu non détecté".to_string(),
            detail: "Ni ufw ni firewalld ne sont installés.".to_string(),
        }),
    }

    let updates_auto = exec::run("dpkg-query", &["-W", "-f", "${Status}", "unattended-upgrades"])
        .map(|captured| captured.text().contains("install ok installed"))
        .unwrap_or(false);
    audit.findings.push(Finding {
        level: if updates_auto { Level::Ok } else { Level::Warning },
        title: "Mises à jour automatiques".to_string(),
        detail: if updates_auto {
            "unattended-upgrades est installé.".to_string()
        } else {
            "unattended-upgrades absent : les correctifs de sécurité attendent une action manuelle."
                .to_string()
        },
    });

    audit.findings.push(Finding {
        level: Level::Info,
        title: "Noyau".to_string(),
        detail: crate::distro::kernel_release(),
    });

    audit.suid_count = suid_count(capabilities);
    audit.findings.push(Finding {
        level: if audit.suid_count > 40 { Level::Warning } else { Level::Ok },
        title: "Binaires SUID".to_string(),
        detail: format!("{} binaires avec le bit setuid dans /usr/bin et /usr/sbin.", audit.suid_count),
    });

    audit.listening_ports = listening_ports(capabilities);
    audit.findings.push(Finding {
        level: Level::Info,
        title: "Ports en écoute".to_string(),
        detail: format!("{} sockets TCP/UDP en écoute.", audit.listening_ports),
    });

    match secure_boot_state() {
        Some(true) => audit.findings.push(Finding {
            level: Level::Ok,
            title: "Secure Boot".to_string(),
            detail: "Activé.".to_string(),
        }),
        Some(false) => audit.findings.push(Finding {
            level: Level::Info,
            title: "Secure Boot".to_string(),
            detail: "Désactivé.".to_string(),
        }),
        None => audit.findings.push(Finding {
            level: Level::Info,
            title: "Secure Boot".to_string(),
            detail: "Non applicable (démarrage BIOS ou variables EFI illisibles).".to_string(),
        }),
    }

    let apparmor = Path::new("/sys/kernel/security/apparmor").exists();
    audit.findings.push(Finding {
        level: if apparmor { Level::Ok } else { Level::Info },
        title: "AppArmor".to_string(),
        detail: if apparmor {
            "Module actif.".to_string()
        } else {
            "Module non monté.".to_string()
        },
    });

    audit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_last_sshd_option() {
        let text = "# PermitRootLogin yes\nPermitRootLogin no\nPasswordAuthentication no\n";
        assert_eq!(sshd_option(text, "PermitRootLogin").as_deref(), Some("no"));
        assert_eq!(sshd_option(text, "PasswordAuthentication").as_deref(), Some("no"));
        assert_eq!(sshd_option(text, "X11Forwarding"), None);
    }

    #[test]
    fn score_drops_with_problems() {
        let audit = Audit {
            findings: vec![
                Finding {
                    level: Level::Ok,
                    title: "ok".into(),
                    detail: String::new(),
                },
                Finding {
                    level: Level::Critical,
                    title: "grave".into(),
                    detail: String::new(),
                },
            ],
            suid_count: 0,
            listening_ports: 0,
        };
        assert_eq!(audit.problems(), 1);
        assert_eq!(audit.score(), 78);
    }
}
