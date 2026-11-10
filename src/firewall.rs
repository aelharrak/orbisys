//! Pare-feu et VPN : état UFW, règles, actions confirmées via Polkit,
//! détection des interfaces VPN (WireGuard, tun/tap, connexions NetworkManager).

use crate::exec;
use serde::Serialize;

/// Une règle de pare-feu.
#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub number: usize,
    pub target: String,
    pub action: String,
    pub source: String,
}

impl Rule {
    /// Empreinte stable d'une règle, indépendante de son numéro.
    pub fn fingerprint(&self) -> String {
        format!("{}|{}|{}", self.target, self.action, self.source)
    }
}

/// État du pare-feu.
#[derive(Debug, Clone, Default)]
pub struct FirewallStatus {
    pub available: bool,
    pub active: bool,
    pub incoming: String,
    pub outgoing: String,
    pub rules: Vec<Rule>,
}

/// Interfaces et connexions VPN détectées.
#[derive(Debug, Clone, Default)]
pub struct VpnStatus {
    pub interfaces: Vec<String>,
    pub wireguard: Vec<String>,
    pub active_connections: Vec<String>,
}

/// Rapport pare-feu complet.
#[derive(Debug, Clone, Default)]
pub struct FirewallReport {
    pub status: FirewallStatus,
    pub vpn: VpnStatus,
    pub errors: Vec<String>,
}

/// Action appliquée au pare-feu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirewallAction {
    Enable,
    Disable,
    Allow(String),
    Deny(String),
    /// Numéro de règle et empreinte de la règle affichée (cible|action|source).
    Delete(usize, String),
}

impl FirewallAction {
    pub fn label(&self) -> String {
        match self {
            FirewallAction::Enable => "Activer le pare-feu".to_string(),
            FirewallAction::Disable => "Désactiver le pare-feu".to_string(),
            FirewallAction::Allow(spec) => format!("Autoriser {spec}"),
            FirewallAction::Deny(spec) => format!("Refuser {spec}"),
            FirewallAction::Delete(number, _) => format!("Supprimer la règle n°{number}"),
        }
    }

    /// Les actions qui coupent un accès demandent une confirmation explicite.
    pub fn needs_confirmation(&self) -> bool {
        matches!(
            self,
            FirewallAction::Disable | FirewallAction::Deny(_) | FirewallAction::Delete(..)
        )
    }
}

/// Une spécification de port valide : `22`, `22/tcp`, `8080/udp`.
pub fn valid_port_spec(spec: &str) -> bool {
    let (port, protocol) = match spec.split_once('/') {
        Some((port, protocol)) => (port, Some(protocol)),
        None => (spec, None),
    };
    if port.is_empty() || port.len() > 6 || !port.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    match port.parse::<u32>() {
        Ok(value) if value >= 1 && value <= 65_535 => {}
        _ => return false,
    }
    match protocol {
        None => true,
        Some("tcp") | Some("udp") => true,
        Some(_) => false,
    }
}

/// Analyse `ufw status numbered`.
pub fn parse_ufw_status(text: &str) -> FirewallStatus {
    let mut status = FirewallStatus {
        available: true,
        ..Default::default()
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Status:") {
            status.active = value.trim().starts_with("active");
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("Default:") {
            // `deny (incoming), allow (outgoing), disabled (routed)`
            for part in value.split(',') {
                let part = part.trim();
                if part.contains("incoming") {
                    status.incoming = part.split_whitespace().next().unwrap_or("").to_string();
                } else if part.contains("outgoing") {
                    status.outgoing = part.split_whitespace().next().unwrap_or("").to_string();
                }
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('[') {
            let Some((number, remainder)) = rest.split_once(']') else {
                continue;
            };
            let Ok(number) = number.trim().parse::<usize>() else {
                continue;
            };
            let fields: Vec<&str> = remainder.split_whitespace().collect();
            if fields.len() < 3 {
                continue;
            }
            let action_index = fields
                .iter()
                .position(|field| *field == "ALLOW" || *field == "DENY" || *field == "REJECT")
                .unwrap_or(1);
            let action = fields
                .get(action_index)
                .map(|value| {
                    let direction = fields.get(action_index + 1).copied().unwrap_or("");
                    format!("{value} {direction}").trim().to_string()
                })
                .unwrap_or_default();
            let source = if action_index + 2 < fields.len() {
                fields[action_index + 2..].join(" ")
            } else {
                String::new()
            };
            status.rules.push(Rule {
                number,
                target: fields[..action_index].join(" "),
                action,
                source,
            });
        }
    }
    status
}

/// Lit l'état UFW (repli sur systemd si les privilèges manquent).
pub fn ufw_status() -> FirewallStatus {
    if !exec::exists("ufw") {
        return FirewallStatus::default();
    }
    if let Ok(captured) = exec::run("ufw", &["status", "numbered"]) {
        if captured.ok {
            return parse_ufw_status(&captured.stdout);
        }
    }
    let active = exec::run("systemctl", &["is-active", "ufw"])
        .map(|captured| captured.text() == "active")
        .unwrap_or(false);
    FirewallStatus {
        available: true,
        active,
        ..Default::default()
    }
}

fn vpn_status() -> VpnStatus {
    let mut status = VpnStatus::default();

    if let Ok(captured) = exec::run("ip", &["-j", "link", "show"]) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&captured.stdout) {
            if let Some(links) = value.as_array() {
                for link in links {
                    let Some(name) = link.get("ifname").and_then(|value| value.as_str()) else {
                        continue;
                    };
                    let kind = link
                        .get("linkinfo")
                        .and_then(|value| value.get("info_kind"))
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    if kind == "wireguard" || kind == "tun" || kind == "tap" {
                        status.interfaces.push(name.to_string());
                    }
                    if name.starts_with("wg") && !status.wireguard.contains(&name.to_string()) {
                        status.wireguard.push(name.to_string());
                    }
                }
            }
        }
    }

    if exec::exists("wg") {
        if let Ok(captured) = exec::run("wg", &["show", "interfaces"]) {
            for name in captured.stdout.split_whitespace() {
                if !status.wireguard.iter().any(|existing| existing == name) {
                    status.wireguard.push(name.to_string());
                }
            }
        }
    }

    if exec::exists("nmcli") {
        if let Ok(captured) = exec::run(
            "nmcli",
            &["-t", "-f", "NAME,TYPE", "connection", "show", "--active"],
        ) {
            for line in captured.stdout.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() >= 2
                    && (fields[1].contains("vpn") || fields[1].contains("wireguard"))
                {
                    status.active_connections.push(fields[0].to_string());
                }
            }
        }
    }

    status
}

/// Rapport pare-feu et VPN.
pub fn report() -> FirewallReport {
    let mut report = FirewallReport {
        status: ufw_status(),
        vpn: vpn_status(),
        errors: Vec::new(),
    };
    if !report.status.available {
        report
            .errors
            .push("ufw n'est pas installé : pare-feu non gérable depuis l'application.".to_string());
    }
    if !report.status.active && report.status.available {
        report
            .errors
            .push("pare-feu inactif : aucune règle n'est appliquée.".to_string());
    }
    report
}

/// Applique une action UFW via Polkit.
pub fn apply(action: &FirewallAction) -> Result<String, String> {
    if !exec::exists("ufw") {
        return Err("ufw n'est pas installé".to_string());
    }
    let args: Vec<String> = match action {
        FirewallAction::Enable => vec!["--force".to_string(), "enable".to_string()],
        FirewallAction::Disable => vec!["disable".to_string()],
        FirewallAction::Allow(spec) => {
            if !valid_port_spec(spec) {
                return Err(format!("port refusé : {spec}"));
            }
            vec!["allow".to_string(), spec.clone()]
        }
        FirewallAction::Deny(spec) => {
            if !valid_port_spec(spec) {
                return Err(format!("port refusé : {spec}"));
            }
            vec!["deny".to_string(), spec.clone()]
        }
        FirewallAction::Delete(number, fingerprint) => {
            if *number == 0 {
                return Err("numéro de règle invalide".to_string());
            }
            // Les numéros UFW changent à chaque ajout ou suppression : on relit
            // les règles et on n'agit que si la règle n° N est bien celle affichée.
            let current = ufw_status();
            match current.rules.iter().find(|rule| rule.number == *number) {
                Some(rule) if rule.fingerprint() == *fingerprint => {}
                _ => {
                    return Err(
                        "les règles ont changé depuis l'affichage : actualisez et recommencez"
                            .to_string(),
                    )
                }
            }
            vec![
                "--force".to_string(),
                "delete".to_string(),
                number.to_string(),
            ]
        }
    };
    exec::run_privileged("ufw", &args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ufw_numbered_status() {
        let text = "Status: active\nLogging: on (low)\nDefault: deny (incoming), allow (outgoing), disabled (routed)\n\n     To                         Action      From\n     --                         ------      ----\n[ 1] 22/tcp                     ALLOW IN    Anywhere\n[ 2] 8080                       DENY IN     192.168.1.0/24\n";
        let status = parse_ufw_status(text);
        assert!(status.available);
        assert!(status.active);
        assert_eq!(status.incoming, "deny");
        assert_eq!(status.outgoing, "allow");
        assert_eq!(status.rules.len(), 2);
        assert_eq!(status.rules[0].target, "22/tcp");
        assert_eq!(status.rules[1].number, 2);
        assert_eq!(status.rules[1].action, "DENY IN");
    }

    #[test]
    fn parses_inactive_status() {
        let status = parse_ufw_status("Status: inactive\n");
        assert!(!status.active);
        assert!(status.rules.is_empty());
    }

    #[test]
    fn validates_port_specifications() {
        assert!(valid_port_spec("22"));
        assert!(valid_port_spec("22/tcp"));
        assert!(valid_port_spec("65535/udp"));
        assert!(!valid_port_spec("0"));
        assert!(!valid_port_spec("70000"));
        assert!(!valid_port_spec("22/sctp"));
        assert!(!valid_port_spec("22; rm -rf /"));
        assert!(!valid_port_spec(""));
    }

    #[test]
    fn dangerous_actions_need_confirmation() {
        assert!(FirewallAction::Disable.needs_confirmation());
        assert!(FirewallAction::Delete(1, "22/tcp|ALLOW IN|Anywhere".into()).needs_confirmation());
        let rule = Rule { number: 2, target: "22/tcp".into(), action: "ALLOW IN".into(), source: "Anywhere".into() };
        assert_eq!(rule.fingerprint(), "22/tcp|ALLOW IN|Anywhere");
        assert!(!FirewallAction::Enable.needs_confirmation());
        assert!(!FirewallAction::Allow("22/tcp".into()).needs_confirmation());
    }
}
