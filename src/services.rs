//! Services systemd : liste, détail et actions (via Polkit).

use crate::exec;
use crate::util;
use serde::Serialize;
use std::collections::HashMap;

/// Une unité de type service.
#[derive(Debug, Clone, Serialize)]
pub struct Service {
    pub unit: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
    pub enabled: String,
}

impl Service {
    pub fn running(&self) -> bool {
        self.active == "active" || self.sub == "running"
    }

    pub fn failed(&self) -> bool {
        self.active == "failed" || self.sub == "failed"
    }
}

/// Actions disponibles sur une unité.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
}

impl ServiceAction {
    pub fn verb(&self) -> &'static str {
        match self {
            ServiceAction::Start => "start",
            ServiceAction::Stop => "stop",
            ServiceAction::Restart => "restart",
            ServiceAction::Enable => "enable",
            ServiceAction::Disable => "disable",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ServiceAction::Start => "Démarrer",
            ServiceAction::Stop => "Arrêter",
            ServiceAction::Restart => "Redémarrer",
            ServiceAction::Enable => "Activer au démarrage",
            ServiceAction::Disable => "Désactiver au démarrage",
        }
    }

    /// Une action de démarrage/arrêt modifie l'état courant ; `enable` non.
    pub fn changes_runtime(&self) -> bool {
        matches!(
            self,
            ServiceAction::Start | ServiceAction::Stop | ServiceAction::Restart
        )
    }
}

/// Liste des services connus de systemd, avec leur état d'activation.
pub fn list() -> Result<Vec<Service>, String> {
    if !exec::exists("systemctl") {
        return Err("systemd non disponible sur ce système".to_string());
    }
    let captured = exec::run_with_timeout(
        "systemctl",
        &[
            "list-units",
            "--type=service",
            "--all",
            "--no-legend",
            "--no-pager",
            "--plain",
        ],
        std::time::Duration::from_secs(30),
    )
    .map_err(|error| error.0)?;
    if !captured.ok {
        return Err(captured.error_text());
    }
    let enabled = enabled_map();
    Ok(parse_units(&captured.stdout, &enabled))
}

/// État d'activation de chaque unité (une seule commande).
pub fn enabled_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(captured) = exec::run_with_timeout(
        "systemctl",
        &["list-unit-files", "--type=service", "--no-legend", "--no-pager"],
        std::time::Duration::from_secs(30),
    ) else {
        return map;
    };
    for line in captured.stdout.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 2 {
            map.insert(fields[0].to_string(), fields[1].to_string());
        }
    }
    map
}

/// Analyse la sortie de `systemctl list-units --plain`.
pub fn parse_units(text: &str, enabled: &HashMap<String, String>) -> Vec<Service> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("●") {
                return None;
            }
            if trimmed.contains("loaded units listed") || trimmed.ends_with("listed.") {
                return None;
            }
            // Les colonnes sont alignées avec plusieurs espaces : on saute les espaces
            // entre champs (un simple `splitn` produisait des champs vides et rejetait
            // l'état dans la description).
            let mut fields: Vec<&str> = Vec::with_capacity(5);
            let mut rest = trimmed;
            for _ in 0..4 {
                let end = rest.find(char::is_whitespace)?;
                fields.push(&rest[..end]);
                rest = rest[end..].trim_start();
            }
            fields.push(rest);
            let unit = fields[0].trim().to_string();
            if !unit.ends_with(".service") {
                return None;
            }
            let enabled_state = enabled.get(&unit).cloned().unwrap_or_else(|| "—".to_string());
            Some(Service {
                unit,
                load: fields[1].to_string(),
                active: fields[2].to_string(),
                sub: fields[3].to_string(),
                description: fields[4].trim().to_string(),
                enabled: enabled_state,
            })
        })
        .collect()
}

/// Détail complet d'une unité.
pub fn details(unit: &str) -> Result<String, String> {
    if !util::valid_unit_name(unit) {
        return Err(format!("unité refusée : {unit}"));
    }
    let captured = exec::run(
        "systemctl",
        &[
            "show",
            unit,
            "--no-pager",
            "-p",
            "Id",
            "-p",
            "Description",
            "-p",
            "ActiveState",
            "-p",
            "SubState",
            "-p",
            "UnitFileState",
            "-p",
            "MainPID",
            "-p",
            "ExecStart",
        ],
    )
    .map_err(|error| error.0)?;
    if caught_failure(&captured) {
        return Err(captured.error_text());
    }
    Ok(captured.text())
}

fn caught_failure(captured: &exec::Captured) -> bool {
    !captured.ok && captured.stdout.trim().is_empty()
}

/// Applique une action à une unité (Polkit si nécessaire).
pub fn action(unit: &str, action: ServiceAction) -> Result<String, String> {
    if !util::valid_unit_name(unit) {
        return Err(format!("unité refusée : {unit}"));
    }
    exec::run_privileged("systemctl", &[action.verb().to_string(), unit.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unit_listing() {
        let mut enabled = HashMap::new();
        enabled.insert("nginx.service".to_string(), "enabled".to_string());
        let text = "  nginx.service loaded active running A high performance web server\n  cron.service loaded failed failed Regular background program\n";
        let services = parse_units(text, &enabled);
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].unit, "nginx.service");
        assert_eq!(services[0].enabled, "enabled");
        assert!(services[0].running());
        assert!(services[1].failed());
    }

    #[test]
    fn parses_columns_aligned_with_several_spaces() {
        // Sortie réelle de `systemctl list-units --plain` : colonnes alignées.
        let text = "  accounts-daemon.service     loaded active   running Accounts Service\n  alsa-state.service          loaded inactive dead    Manage Sound Card State\n";
        let services = parse_units(text, &HashMap::new());
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].load, "loaded");
        assert_eq!(services[0].active, "active");
        assert_eq!(services[0].sub, "running");
        assert_eq!(services[0].description, "Accounts Service");
        assert_eq!(services[1].sub, "dead");
        assert_eq!(services[1].description, "Manage Sound Card State");
    }

    #[test]
    fn rejects_invalid_unit_names() {
        assert!(action("--now", ServiceAction::Stop).is_err());
        assert!(details("a;b").is_err());
    }
}
