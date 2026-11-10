//! Sauvegardes systemd : timers, unités de sauvegarde, dernières exécutions
//! et échecs.

use crate::exec;
use crate::util;
use serde::Serialize;

/// Un timer systemd.
#[derive(Debug, Clone, Serialize)]
pub struct Timer {
    pub unit: String,
    pub activates: String,
    pub next: String,
    pub left: String,
    pub last: String,
    pub passed: String,
}

/// Une unité de sauvegarde.
#[derive(Debug, Clone, Serialize)]
pub struct BackupUnit {
    pub name: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

impl BackupUnit {
    pub fn failed(&self) -> bool {
        self.active == "failed" || self.sub == "failed"
    }
}

/// Rapport des sauvegardes systemd.
#[derive(Debug, Clone, Default)]
pub struct TimersReport {
    pub timers: Vec<Timer>,
    pub units: Vec<BackupUnit>,
    pub errors: Vec<String>,
}

impl TimersReport {
    pub fn failed(&self) -> usize {
        self.units.iter().filter(|unit| unit.failed()).count()
    }

    /// Timer associé à une unité de sauvegarde.
    pub fn timer_for(&self, unit: &str) -> Option<&Timer> {
        self.timers
            .iter()
            .find(|timer| timer.activates == unit || timer.unit == unit)
    }
}

/// Mots-clés reconnaissant une unité de sauvegarde.
pub const BACKUP_KEYWORDS: [&str; 7] = [
    "backup",
    "restic",
    "borg",
    "duplicity",
    "rsnapshot",
    "timeshift",
    "snapshot",
];

/// Une unité évoque-t-elle une sauvegarde ?
pub fn is_backup_unit(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    BACKUP_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword))
}

/// Analyse `systemctl list-timers --all --no-legend`.
pub fn parse_timers(text: &str) -> Vec<Timer> {
    let mut timers: Vec<Timer> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.contains("timers listed") || trimmed.starts_with("NEXT") {
            continue;
        }
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let Some(position) = tokens.iter().position(|token| token.ends_with(".timer")) else {
            continue;
        };
        let unit = tokens[position].to_string();
        if !util::valid_unit_name(&unit) {
            continue;
        }
        let activates = tokens
            .get(position + 1)
            .map(|value| value.trim_end_matches('.').to_string())
            .unwrap_or_default();
        // Les colonnes de dates contiennent des espaces : les quatre premiers
        // jetons forment la date suivante, le cinquième le délai restant.
        let (next, left, last, passed) = if tokens[0] == "n/a" {
            ("n/a".to_string(), String::new(), "n/a".to_string(), String::new())
        } else if position >= 9 {
            (
                tokens[0..4].join(" "),
                tokens[4].to_string(),
                tokens[5..9].join(" "),
                tokens[9..position].join(" "),
            )
        } else {
            (tokens[0..position].join(" "), String::new(), String::new(), String::new())
        };
        timers.push(Timer {
            unit,
            activates,
            next,
            left,
            last,
            passed,
        });
    }
    timers.sort_by(|a, b| a.unit.cmp(&b.unit));
    timers
}

/// Analyse `systemctl list-units --type=service --all --no-legend`.
pub fn parse_service_units(text: &str) -> Vec<BackupUnit> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('●') || trimmed.contains("loaded units listed") {
                return None;
            }
            let fields: Vec<&str> = trimmed.splitn(5, char::is_whitespace).collect();
            if fields.len() < 5 {
                return None;
            }
            let name = fields[0].trim();
            if !name.ends_with(".service") || !is_backup_unit(name) {
                return None;
            }
            if !util::valid_unit_name(name) {
                return None;
            }
            Some(BackupUnit {
                name: name.to_string(),
                active: fields[2].to_string(),
                sub: fields[3].to_string(),
                description: fields[4].trim().to_string(),
            })
        })
        .collect()
}

/// Liste les timers.
pub fn timers() -> Result<Vec<Timer>, String> {
    let captured = exec::run_with_timeout(
        "systemctl",
        &["list-timers", "--all", "--no-legend", "--no-pager"],
        std::time::Duration::from_secs(30),
    )
    .map_err(|error| error.0)?;
    if !captured.ok && captured.stdout.trim().is_empty() {
        return Err(captured.error_text());
    }
    Ok(parse_timers(&captured.stdout))
}

/// Liste les unités de sauvegarde connues.
pub fn backup_units() -> Vec<BackupUnit> {
    exec::run_with_timeout(
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
    .map(|captured| parse_service_units(&captured.stdout))
    .unwrap_or_default()
}

/// Démarre, arrête ou active un timer/une unité.
pub fn control(unit: &str, action: &str) -> Result<String, String> {
    if !util::valid_unit_name(unit) {
        return Err(format!("unité refusée : {unit}"));
    }
    if !matches!(action, "start" | "stop" | "restart" | "enable" | "disable") {
        return Err(format!("action refusée : {action}"));
    }
    exec::run_privileged("systemctl", &[action.to_string(), unit.to_string()])?;
    Ok(format!("{action} : {unit}"))
}

/// Collecte le rapport.
pub fn report() -> TimersReport {
    let mut report = TimersReport::default();
    match timers() {
        Ok(list) => report.timers = list,
        Err(error) => report.errors.push(format!("timers : {error}")),
    }
    report.units = backup_units();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timer_listing() {
        let text = "Mon 2026-11-02 10:00:00 CET 5min Mon 2026-11-02 09:55:00 CET 5min ago apt-daily.timer apt-daily.service\nn/a n/a n/a backup.timer backup.service\n";
        let timers = parse_timers(text);
        assert_eq!(timers.len(), 2);
        assert_eq!(timers[0].unit, "apt-daily.timer");
        assert_eq!(timers[0].activates, "apt-daily.service");
        assert_eq!(timers[0].left, "5min");
        assert_eq!(timers[0].passed, "5min ago");
        assert_eq!(timers[1].unit, "backup.timer");
        assert_eq!(timers[1].next, "n/a");
    }

    #[test]
    fn ignores_summary_lines() {
        assert!(parse_timers("2 timers listed.\n").is_empty());
    }

    #[test]
    fn recognizes_backup_units() {
        assert!(is_backup_unit("restic-backup.service"));
        assert!(is_backup_unit("borg.timer"));
        assert!(is_backup_unit("timeshift-nightly.service"));
        assert!(!is_backup_unit("nginx.service"));
    }

    #[test]
    fn parses_backup_service_units() {
        let text = "  restic-backup.service loaded active running Restic nightly backup\n  nginx.service loaded active running A web server\n  borg.service loaded failed failed Borg archive\n";
        let units = parse_service_units(text);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].name, "restic-backup.service");
        assert!(!units[0].failed());
        assert!(units[1].failed());
    }

    #[test]
    fn links_timer_to_unit() {
        let report = TimersReport {
            timers: vec![Timer {
                unit: "backup.timer".to_string(),
                activates: "backup.service".to_string(),
                next: "n/a".to_string(),
                left: String::new(),
                last: "n/a".to_string(),
                passed: String::new(),
            }],
            ..TimersReport::default()
        };
        assert!(report.timer_for("backup.service").is_some());
        assert!(report.timer_for("autre.service").is_none());
        assert!(control("a;b", "start").is_err());
        assert!(control("ok.service", "mask").is_err());
    }
}
