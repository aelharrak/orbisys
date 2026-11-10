//! Lecture des journaux systemd, filtrée par unité et par priorité.

use crate::exec;
use crate::util;

/// Niveau de priorité `journalctl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    All,
    Errors,
    Warnings,
    Info,
}

impl Priority {
    pub fn label(&self) -> &'static str {
        match self {
            Priority::All => "Tout",
            Priority::Errors => "Erreurs",
            Priority::Warnings => "Avertissements",
            Priority::Info => "Informations",
        }
    }

    fn journal_level(&self) -> Option<&'static str> {
        match self {
            Priority::All => None,
            Priority::Errors => Some("err"),
            Priority::Warnings => Some("warning"),
            Priority::Info => Some("info"),
        }
    }
}

/// Lit les `count` dernières lignes du journal.
pub fn read(count: usize, unit: Option<&str>, priority: Priority) -> Result<String, String> {
    if !exec::exists("journalctl") {
        return Err("journalctl absent : journaux indisponibles".to_string());
    }
    if let Some(unit) = unit {
        if !util::valid_unit_name(unit) {
            return Err(format!("unité refusée : {unit}"));
        }
    }
    let count = count.clamp(1, 5_000).to_string();
    let mut args: Vec<String> = vec![
        "-n".to_string(),
        count,
        "--no-pager".to_string(),
        "-o".to_string(),
        "short-iso".to_string(),
    ];
    if let Some(level) = priority.journal_level() {
        args.push("-p".to_string());
        args.push(level.to_string());
    }
    if let Some(unit) = unit {
        args.push("-u".to_string());
        args.push(unit.to_string());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let captured = exec::run_with_timeout(
        "journalctl",
        &refs,
        std::time::Duration::from_secs(45),
    )
    .map_err(|error| error.0)?;
    if !captured.ok && captured.stdout.trim().is_empty() {
        return Err(captured.error_text());
    }
    Ok(captured.stdout)
}

/// Nombre de lignes d'un texte de journal.
pub fn line_count(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty()).count()
}
