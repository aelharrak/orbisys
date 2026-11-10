//! Journaux avancés : recherche multi-unités, plage de dates, motif et
//! analyse des messages d'erreur récurrents.

use crate::exec;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Une requête sur le journal systemd.
#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub units: Vec<String>,
    pub since: String,
    pub until: String,
    pub priority: String,
    pub grep: String,
    pub limit: usize,
}

/// Un message d'erreur et son nombre d'occurrences.
#[derive(Debug, Clone, Serialize)]
pub struct RecurringError {
    pub message: String,
    pub count: usize,
}

/// Priorités acceptées par `journalctl -p`.
pub const PRIORITIES: [&str; 5] = ["emerg", "err", "warning", "notice", "info"];

/// Découpe une liste d'unités séparées par des virgules ou des espaces.
pub fn parse_units(text: &str) -> Vec<String> {
    let mut units: Vec<String> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|unit| !unit.is_empty())
        .map(str::to_string)
        .collect();
    units.retain(|unit| crate::util::valid_unit_name(unit));
    // `dedup()` ne retire que les doublons consécutifs : on garde l'ordre
    // d'apparition avec un ensemble de clés déjà vues.
    let mut seen = std::collections::HashSet::new();
    units.retain(|unit| seen.insert(unit.clone()));
    units
}

/// Une date `AAAA-MM-JJ` ou `AAAA-MM-JJ HH:MM` (vide accepté).
pub fn valid_date(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return true;
    }
    let (date, time) = match text.split_once(' ') {
        Some((date, time)) => (date, Some(time.trim())),
        None => (text, None),
    };
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || !parts.iter().all(|part| part.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    let month: u32 = parts[1].parse().unwrap_or(0);
    let day: u32 = parts[2].parse().unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    if let Some(time) = time {
        let fields: Vec<&str> = time.split(':').collect();
        if fields.len() != 2
            || fields.iter().any(|field| field.len() != 2)
            || !fields.iter().all(|field| field.chars().all(|c| c.is_ascii_digit()))
        {
            return false;
        }
        let hour: u32 = fields[0].parse().unwrap_or(99);
        let minute: u32 = fields[1].parse().unwrap_or(99);
        if hour > 23 || minute > 59 {
            return false;
        }
    }
    true
}

/// Construit les arguments `journalctl` de la requête.
pub fn journal_args(query: &LogQuery) -> Vec<String> {
    let mut args = vec!["--no-pager".to_string(), "-o".to_string(), "short-iso".to_string()];
    for unit in &query.units {
        args.push("-u".to_string());
        args.push(unit.clone());
    }
    if !query.since.trim().is_empty() {
        args.push("--since".to_string());
        args.push(query.since.trim().to_string());
    }
    if !query.until.trim().is_empty() {
        args.push("--until".to_string());
        args.push(query.until.trim().to_string());
    }
    if PRIORITIES.contains(&query.priority.as_str()) {
        args.push("-p".to_string());
        args.push(query.priority.clone());
    }
    if !query.grep.trim().is_empty() {
        args.push("--grep".to_string());
        args.push(query.grep.trim().to_string());
    }
    let limit = query.limit.clamp(10, 5_000);
    args.push("-n".to_string());
    args.push(limit.to_string());
    args
}

/// Exécute la requête et renvoie le texte du journal.
pub fn search(query: &LogQuery) -> Result<String, String> {
    if !query.since.is_empty() && !valid_date(&query.since) {
        return Err("date de début invalide : AAAA-MM-JJ ou AAAA-MM-JJ HH:MM".to_string());
    }
    if !query.until.is_empty() && !valid_date(&query.until) {
        return Err("date de fin invalide : AAAA-MM-JJ ou AAAA-MM-JJ HH:MM".to_string());
    }
    if !query.grep.is_empty() && query.grep.len() > 200 {
        return Err("motif trop long".to_string());
    }
    let args = journal_args(query);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let captured = exec::run_with_timeout("journalctl", &refs, std::time::Duration::from_secs(120))
        .map_err(|error| error.0)?;
    if captured.ok {
        Ok(captured.text())
    } else {
        Err(captured.error_text())
    }
}

/// Normalise un message pour regrouper les variantes (PID, adresses, nombres).
pub fn normalize_message(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_token = false;
    for token in line.split_whitespace() {
        let digits = token.chars().filter(|c| c.is_ascii_digit()).count();
        let looks_numeric = !token.is_empty() && digits * 2 >= token.chars().count();
        let cleaned = if looks_numeric {
            "N".to_string()
        } else {
            token.to_string()
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&cleaned);
        in_token = true;
    }
    if !in_token {
        return String::new();
    }
    out
}

/// Compte les messages récurrents, les plus fréquents d'abord.
pub fn top_messages(text: &str, limit: usize) -> Vec<RecurringError> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for line in text.lines() {
        // `short-iso` : « 2026-11-02T09:12:33+0100 hôte unit[pid]: message ».
        let message = match line.split_once("]: ") {
            Some((_, message)) => message,
            None => line
                .split_once(": ")
                .map(|(_, message)| message)
                .unwrap_or(line),
        };
        let normalized = normalize_message(message.trim());
        if normalized.is_empty() {
            continue;
        }
        *counts.entry(normalized).or_insert(0) += 1;
    }
    let mut errors: Vec<RecurringError> = counts
        .into_iter()
        .map(|(message, count)| RecurringError { message, count })
        .collect();
    errors.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.message.cmp(&b.message)));
    errors.truncate(limit.max(1));
    errors
}

/// Analyse les erreurs des dernières 24 heures.
pub fn recurring_errors(limit: usize) -> Result<Vec<RecurringError>, String> {
    let args = [
        "--no-pager",
        "-o",
        "short-iso",
        "-p",
        "err",
        "--since",
        "24 hours ago",
    ];
    let captured = exec::run_with_timeout("journalctl", &args, std::time::Duration::from_secs(120))
        .map_err(|error| error.0)?;
    if !captured.ok {
        return Err(captured.error_text());
    }
    Ok(top_messages(&captured.stdout, limit))
}

/// Écrit un extrait de journal dans un fichier.
pub fn export(text: &str, path: &Path) -> Result<(), String> {
    if text.is_empty() {
        return Err("rien à exporter : lancez d'abord une recherche".to_string());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, text).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> LogQuery {
        LogQuery {
            units: vec!["nginx.service".to_string(), "sshd.service".to_string()],
            since: "2026-11-01".to_string(),
            until: String::new(),
            priority: "err".to_string(),
            grep: String::new(),
            limit: 200,
        }
    }

    #[test]
    fn builds_journal_arguments() {
        let args = journal_args(&query());
        assert_eq!(args.iter().filter(|arg| *arg == "-u").count(), 2);
        assert!(args.contains(&"--since".to_string()));
        assert!(args.contains(&"2026-11-01".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert_eq!(args.last().unwrap(), "200");
    }

    #[test]
    fn clamps_and_filters_limit() {
        let mut q = query();
        q.limit = 1_000_000;
        assert_eq!(journal_args(&q).last().unwrap(), "5000");
        q.limit = 1;
        assert_eq!(journal_args(&q).last().unwrap(), "10");
    }

    #[test]
    fn ignores_unknown_priority() {
        let mut q = query();
        q.priority = "err; rm -rf /".to_string();
        let args = journal_args(&q);
        assert!(!args.contains(&"-p".to_string()));
    }

    #[test]
    fn validates_dates() {
        assert!(valid_date(""));
        assert!(valid_date("2026-11-02"));
        assert!(valid_date("2026-11-02 09:30"));
        assert!(!valid_date("02/11/2026"));
        assert!(!valid_date("2026-13-02"));
        assert!(!valid_date("2026-11-02 25:00"));
        assert!(!valid_date("2026-11-02; reboot"));
    }

    #[test]
    fn parses_unit_lists() {
        let units = parse_units("nginx.service, sshd.service  nginx.service bad;unit");
        assert_eq!(units, vec!["nginx.service", "sshd.service"]);
    }

    #[test]
    fn normalizes_numbers_and_groups_messages() {
        assert_eq!(
            normalize_message("Connection refused from 10.0.0.5:22"),
            "Connection refused from N"
        );
        let text = "2026-11-02T09:12:33+0100 hôte nginx[123]: worker 42 crashed\n2026-11-02T09:13:01+0100 hôte nginx[124]: worker 43 crashed\n";
        let top = top_messages(text, 5);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].count, 2);
    }

    #[test]
    fn rejects_empty_export() {
        let path = std::env::temp_dir().join("um-journal-test.txt");
        assert!(export("", &path).is_err());
    }

    #[test]
    fn rejects_bad_dates_in_search() {
        let mut q = query();
        q.since = "hier".to_string();
        assert!(search(&q).is_err());
    }
}
