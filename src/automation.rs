//! Automatisation : tâches cron de l'utilisateur et minuteurs systemd.

use crate::exec;
use crate::journal;
use crate::panel::{load_json, save_json};
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Une tâche cron.
#[derive(Debug, Clone, Serialize)]
pub struct CronJob {
    pub schedule: String,
    pub command: String,
}

/// Un minuteur systemd.
#[derive(Debug, Clone, Serialize)]
pub struct Timer {
    pub unit: String,
    pub state: String,
}

/// État de la page Automatisation.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationReport {
    pub jobs: Vec<CronJob>,
    pub timers: Vec<Timer>,
    pub crontab_available: bool,
    pub systemd_available: bool,
}

const SPECIAL_SCHEDULES: [&str; 9] = [
    "@reboot",
    "@yearly",
    "@annually",
    "@monthly",
    "@weekly",
    "@daily",
    "@midnight",
    "@hourly",
    "@startup",
];

/// Une planification cron valide (cinq champs ou `@daily`…).
pub fn valid_schedule(schedule: &str) -> bool {
    let trimmed = schedule.trim();
    if SPECIAL_SCHEDULES.contains(&trimmed) {
        return true;
    }
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    if fields.len() != 5 {
        return false;
    }
    fields.iter().all(|field| {
        !field.is_empty()
            && field
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '*' | '/' | ',' | '-'))
    })
}

/// Une commande cron acceptable (une seule ligne, longueur bornée).
pub fn valid_command(command: &str) -> bool {
    let trimmed = command.trim();
    !trimmed.is_empty() && trimmed.len() <= 1000 && !trimmed.contains('\n') && !trimmed.contains('\r')
}

/// Analyse un crontab (`crontab -l`).
pub fn parse_crontab(text: &str) -> Vec<CronJob> {
    let mut jobs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Les affectations d'environnement (`PATH=/…`) ne sont pas des tâches.
        if !line.starts_with('@') && line.contains('=') && !line.contains(' ') {
            continue;
        }
        let mut parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let schedule = if parts[0].starts_with('@') {
            parts.remove(0).to_string()
        } else {
            if parts.len() < 6 {
                continue;
            }
            let schedule = parts[..5].join(" ");
            parts = parts[5..].to_vec();
            schedule
        };
        if !valid_schedule(&schedule) {
            continue;
        }
        let command = parts.join(" ");
        if command.is_empty() {
            continue;
        }
        jobs.push(CronJob { schedule, command });
    }
    jobs
}

/// Reconstruit un crontab sans la tâche indiquée (conserve commentaires et
/// variables d'environnement).
pub fn remove_from_crontab(text: &str, schedule: &str, command: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() && kept.is_empty() {
            continue;
        }
        let is_target = trimmed.starts_with(schedule) && trimmed.ends_with(command);
        if !is_target {
            kept.push(line.to_string());
        }
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Ajoute une tâche au crontab de l'utilisateur.
pub fn add_job(schedule: &str, command: &str) -> Result<String, String> {
    if !valid_schedule(schedule) {
        return Err("planification invalide : cinq champs (min heure jour mois jour-semaine) ou @daily".to_string());
    }
    if !valid_command(command) {
        return Err("commande invalide".to_string());
    }
    if !exec::exists("crontab") {
        return Err("crontab n'est pas installé".to_string());
    }
    let current = exec::run("crontab", &["-l"])
        .map(|captured| captured.stdout)
        .unwrap_or_default();
    let mut content = current.clone();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("{schedule} {command}\n"));
    write_crontab(&content)?;
    Ok(format!("tâche ajoutée : {schedule} {command}"))
}

/// Supprime une tâche du crontab de l'utilisateur.
pub fn remove_job(schedule: &str, command: &str) -> Result<String, String> {
    if !valid_schedule(schedule) || !valid_command(command) {
        return Err("tâche invalide".to_string());
    }
    let current = exec::run("crontab", &["-l"])
        .map(|captured| captured.stdout)
        .unwrap_or_default();
    let updated = remove_from_crontab(&current, schedule, command);
    write_crontab(&updated)?;
    Ok(format!("tâche supprimée : {schedule} {command}"))
}

fn write_crontab(content: &str) -> Result<(), String> {
    let temporary = std::env::temp_dir().join(format!("um-crontab-{}", std::process::id()));
    std::fs::write(&temporary, content).map_err(|error| error.to_string())?;
    let captured = exec::run("crontab", &[temporary.to_string_lossy().as_ref()])
        .map_err(|error| error.0);
    let _ = std::fs::remove_file(&temporary);
    let captured = captured?;
    if captured.ok {
        Ok(())
    } else {
        Err(captured.error_text())
    }
}

/// Analyse `systemctl list-unit-files --type=timer`.
pub fn parse_timers(text: &str) -> Vec<Timer> {
    let mut timers = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 || !fields[0].ends_with(".timer") {
            continue;
        }
        timers.push(Timer {
            unit: fields[0].to_string(),
            state: fields[1].to_string(),
        });
    }
    timers.sort_by(|a, b| a.unit.cmp(&b.unit));
    timers
}

/// Active ou désactive un minuteur systemd.
pub fn set_timer(unit: &str, enabled: bool) -> Result<String, String> {
    if !util::valid_unit_name(unit) || !unit.ends_with(".timer") {
        return Err("minuteur invalide : nom attendu au format nom.timer".to_string());
    }
    let action = if enabled { "enable" } else { "disable" };
    exec::run_privileged(
        "systemctl",
        &[action.to_string(), "--now".to_string(), unit.to_string()],
    )?;
    Ok(format!(
        "minuteur {unit} {}",
        if enabled { "activé" } else { "désactivé" }
    ))
}

/// Lit l'état de l'automatisation.
pub fn report() -> AutomationReport {
    let crontab_available = exec::exists("crontab");
    let jobs = if crontab_available {
        exec::run("crontab", &["-l"])
            .map(|captured| parse_crontab(&captured.stdout))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let systemd_available = exec::exists("systemctl");
    let timers = if systemd_available {
        exec::run(
            "systemctl",
            &[
                "list-unit-files",
                "--type=timer",
                "--no-pager",
                "--no-legend",
            ],
        )
        .map(|captured| parse_timers(&captured.stdout))
        .unwrap_or_default()
    } else {
        Vec::new()
    };
    AutomationReport {
        jobs,
        timers,
        crontab_available,
        systemd_available,
    }
}

// ---------------------------------------------------------------------------
// Règles de seuil (indicateur ≥ seuil → action)
// ---------------------------------------------------------------------------

/// Nombre maximal de règles conservées.
pub const MAX_RULES: usize = 50;
/// Fichier des règles (`~/.local/share/orbisys`).
const RULES_FILE: &str = "automation_rules.json";
/// Fichier des derniers déclenchements (anti-répétition).
const RULE_STATE_FILE: &str = "automation_state.json";
/// Délai minimal entre deux déclenchements d'une même règle (secondes).
const RULE_COOLDOWN: u64 = 600;

/// Indicateurs surveillés : (clé, libellé, unité).
pub const METRICS: [(&str, &str, &str); 4] = [
    ("cpu", "Processeur", "%"),
    ("mem", "Mémoire", "%"),
    ("disk", "Disque", "%"),
    ("temp", "Température", "°C"),
];

/// Actions déclenchées : (clé, libellé).
pub const ACTIONS: [(&str, &str); 2] = [
    ("notify", "Notification de bureau"),
    ("record", "Enregistrer une mesure"),
];

/// Une règle : lorsque l'indicateur atteint le seuil, l'action est exécutée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub metric: String,
    pub threshold: u8,
    pub action: String,
}

impl Rule {
    /// Libellé lisible (« Processeur ≥ 90 % »).
    pub fn label(&self) -> String {
        format!(
            "{} ≥ {} {}",
            metric_label(&self.metric),
            self.threshold,
            metric_unit(&self.metric)
        )
    }
}

/// Libellé d'un indicateur (la clé elle-même s'il est inconnu).
pub fn metric_label(metric: &str) -> &str {
    METRICS
        .iter()
        .find(|(key, _, _)| *key == metric)
        .map(|(_, label, _)| *label)
        .unwrap_or(metric)
}

/// Unité d'un indicateur.
pub fn metric_unit(metric: &str) -> &str {
    METRICS
        .iter()
        .find(|(key, _, _)| *key == metric)
        .map(|(_, _, unit)| *unit)
        .unwrap_or("")
}

/// Libellé d'une action.
pub fn action_label(action: &str) -> &str {
    ACTIONS
        .iter()
        .find(|(key, _)| *key == action)
        .map(|(_, label)| *label)
        .unwrap_or(action)
}

/// Une règle connue, avec un seuil plausible.
pub fn valid_rule(rule: &Rule) -> bool {
    METRICS.iter().any(|(key, _, _)| *key == rule.metric)
        && ACTIONS.iter().any(|(key, _)| *key == rule.action)
        && (1..=120).contains(&rule.threshold)
}

/// Règles enregistrées (les entrées illisibles sont ignorées).
pub fn load_rules() -> Vec<Rule> {
    let mut rules: Vec<Rule> = load_json(RULES_FILE).unwrap_or_default();
    rules.retain(valid_rule);
    rules
}

/// Enregistre la liste complète des règles.
pub fn save_rules(rules: &[Rule]) -> Result<(), String> {
    save_json(RULES_FILE, &rules.to_vec())
}

/// Ajoute une règle (refuse les doublons et au-delà de `MAX_RULES`).
pub fn add_rule(rule: Rule) -> Result<String, String> {
    if !valid_rule(&rule) {
        return Err("règle invalide : indicateur, seuil (1 à 120) et action attendus".to_string());
    }
    let mut rules = load_rules();
    if rules.iter().any(|existing| existing == &rule) {
        return Err("cette règle existe déjà".to_string());
    }
    if rules.len() >= MAX_RULES {
        return Err(format!("maximum de {MAX_RULES} règles atteint"));
    }
    let label = rule.label();
    rules.push(rule);
    save_rules(&rules)?;
    Ok(format!("règle ajoutée : {label}"))
}

/// Supprime la règle d'indice `index`.
pub fn remove_rule(index: usize) -> Result<String, String> {
    let mut rules = load_rules();
    if index >= rules.len() {
        return Err("règle introuvable".to_string());
    }
    let removed = rules.remove(index);
    save_rules(&rules)?;
    Ok(format!("règle supprimée : {}", removed.label()))
}

/// Règles déclenchées par une mesure (sans effet de bord).
pub fn triggered<'a>(rules: &'a [Rule], measure: &crate::p_monitor::Measure) -> Vec<&'a Rule> {
    rules
        .iter()
        .filter(|rule| {
            value_for(&rule.metric, measure)
                .is_some_and(|value| value >= f32::from(rule.threshold))
        })
        .collect()
}

fn value_for(metric: &str, measure: &crate::p_monitor::Measure) -> Option<f32> {
    match metric {
        "cpu" => Some(measure.cpu),
        "mem" => Some(measure.mem),
        "disk" => Some(measure.disk),
        "temp" => measure.temp,
        _ => None,
    }
}

/// Message d'une règle déclenchée.
fn rule_message(rule: &Rule, value: f32) -> String {
    format!(
        "{} à {value:.0} {} (seuil {})",
        metric_label(&rule.metric),
        metric_unit(&rule.metric),
        rule.threshold
    )
}

/// Vérifie les règles maintenant : notification ou mesure enregistrée.
///
/// Une même règle ne se déclenche qu'une fois par `RULE_COOLDOWN` secondes.
pub fn check_now() -> Vec<String> {
    let rules = load_rules();
    if rules.is_empty() {
        return Vec::new();
    }
    let measure = crate::p_monitor::measure_now();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let mut state: HashMap<String, u64> = load_json(RULE_STATE_FILE).unwrap_or_default();
    let mut fired = Vec::new();
    let mut changed = false;
    for rule in triggered(&rules, &measure) {
        let key = format!("{}:{}:{}", rule.metric, rule.threshold, rule.action);
        if state
            .get(&key)
            .is_some_and(|last| now.saturating_sub(*last) < RULE_COOLDOWN)
        {
            continue;
        }
        let message = rule_message(rule, value_for(&rule.metric, &measure).unwrap_or(0.0));
        match rule.action.as_str() {
            "notify" => {
                let _ = notify(&message);
            }
            "record" => journal::record("règle de seuil", &rule.metric, true, &message),
            _ => {}
        }
        state.insert(key, now);
        changed = true;
        fired.push(message);
    }
    if changed {
        let _ = save_json(RULE_STATE_FILE, &state);
    }
    fired
}

/// Notification de bureau si `notify-send` est disponible.
pub fn notify(message: &str) -> Result<String, String> {
    if !exec::exists("notify-send") {
        return Err("notify-send n'est pas disponible".to_string());
    }
    let captured = exec::run("notify-send", &["OrbiSys", message]).map_err(|error| error.0)?;
    if captured.ok {
        Ok(message.to_string())
    } else {
        Err(captured.error_text())
    }
}

/// Envoie une notification de test.
pub fn test_notification() -> Result<String, String> {
    notify("Notification de test des règles de seuil.")?;
    Ok("notification de test envoyée".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_schedules() {
        assert!(valid_schedule("*/5 * * * *"));
        assert!(valid_schedule("0 3 * * 1-5"));
        assert!(valid_schedule("@daily"));
        assert!(valid_schedule("@reboot"));
        assert!(!valid_schedule("0 3 * *"));
        assert!(!valid_schedule("chaque jour"));
        assert!(!valid_schedule("0 3 * * *; rm -rf /"));
    }

    #[test]
    fn validates_commands() {
        assert!(valid_command("/usr/bin/rsync -a /home /backup"));
        assert!(!valid_command(""));
        assert!(!valid_command("a\nb"));
    }

    #[test]
    fn parses_crontab_with_env_and_comments() {
        let text = "# m h dom mon dow command\nPATH=/usr/local/bin:/usr/bin\n*/5 * * * * /usr/bin/backup.sh\n@daily /usr/bin/cleanup\n";
        let jobs = parse_crontab(text);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].schedule, "*/5 * * * *");
        assert_eq!(jobs[0].command, "/usr/bin/backup.sh");
        assert_eq!(jobs[1].schedule, "@daily");
    }

    #[test]
    fn removes_only_the_targeted_job() {
        let text = "# tâches\n0 3 * * * /usr/bin/a\n0 4 * * * /usr/bin/b\n";
        let updated = remove_from_crontab(text, "0 3 * * *", "/usr/bin/a");
        assert!(!updated.contains("/usr/bin/a"));
        assert!(updated.contains("/usr/bin/b"));
        assert!(updated.contains("# tâches"));
    }

    #[test]
    fn parses_systemd_timers() {
        let text = "apt-daily.timer enabled enabled\napt-daily-upgrade.timer disabled disabled\nlogrotate.timer enabled enabled\n";
        let timers = parse_timers(text);
        assert_eq!(timers.len(), 3);
        assert_eq!(timers[0].unit, "apt-daily-upgrade.timer");
        assert_eq!(timers[2].state, "enabled");
    }

    #[test]
    fn rejects_bad_timer_names() {
        assert!(set_timer("rm -rf /", true).is_err());
        assert!(set_timer("apt-daily.service", true).is_err());
        assert!(set_timer("a;b.timer", true).is_err());
    }

    #[test]
    fn rules_are_validated() {
        let rule = |metric: &str, threshold: u8, action: &str| Rule {
            metric: metric.to_string(),
            threshold,
            action: action.to_string(),
        };
        assert!(valid_rule(&rule("cpu", 90, "notify")));
        assert!(valid_rule(&rule("temp", 80, "record")));
        assert!(!valid_rule(&rule("gpu", 90, "notify")));
        assert!(!valid_rule(&rule("cpu", 0, "notify")));
        assert!(!valid_rule(&rule("cpu", 121, "notify")));
        assert!(!valid_rule(&rule("cpu", 90, "root")));
    }

    #[test]
    fn triggers_rules_above_threshold() {
        let measure = crate::p_monitor::Measure {
            cpu: 95.0,
            mem: 40.0,
            disk: 51.0,
            temp: None,
        };
        let rules = vec![
            Rule {
                metric: "cpu".into(),
                threshold: 90,
                action: "notify".into(),
            },
            Rule {
                metric: "mem".into(),
                threshold: 90,
                action: "notify".into(),
            },
            Rule {
                metric: "temp".into(),
                threshold: 70,
                action: "record".into(),
            },
        ];
        let hits = triggered(&rules, &measure);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].metric, "cpu");
    }

    #[test]
    fn rule_labels_include_unit() {
        let rule = Rule {
            metric: "temp".into(),
            threshold: 80,
            action: "notify".into(),
        };
        assert_eq!(rule.label(), "Température ≥ 80 °C");
        assert_eq!(
            rule_message(&rule, 83.4),
            "Température à 83 °C (seuil 80)"
        );
    }
}
