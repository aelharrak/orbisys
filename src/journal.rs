//! Journal des actions : chaque opération est horodatée et rejouable en CSV.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Une action enregistrée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub timestamp: String,
    pub action: String,
    pub target: String,
    pub ok: bool,
    pub detail: String,
}

/// Emplacement du journal (`$XDG_STATE_HOME/orbisys/journal.jsonl`).
pub fn path() -> PathBuf {
    let base = dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("orbisys").join("journal.jsonl")
}

/// Enregistre une action. Une erreur d'écriture ne doit jamais faire échouer
/// l'action elle-même : elle est ignorée silencieusement.
pub fn record(action: &str, target: &str, ok: bool, detail: &str) {
    let entry = Entry {
        timestamp: crate::util::now_human(),
        action: action.to_string(),
        target: target.to_string(),
        ok,
        detail: crate::util::ellipsis(detail.trim(), 400),
    };
    let path = path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(line) = serde_json::to_string(&entry) {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// Les `limit` dernières entrées, de la plus récente à la plus ancienne.
pub fn recent(limit: usize) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
        .collect();
    entries.reverse();
    entries.truncate(limit);
    entries
}

/// Export CSV du journal.
pub fn to_csv(entries: &[Entry]) -> String {
    let mut out = String::from("horodatage;action;cible;succes;detail\n");
    for entry in entries {
        out.push_str(&format!(
            "{};{};{};{};{}\n",
            entry.timestamp,
            entry.action,
            entry.target,
            if entry.ok { "oui" } else { "non" },
            entry.detail.replace(';', ",").replace('\n', " ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_path_is_absolute() {
        assert!(path().is_absolute());
        assert!(path().to_string_lossy().ends_with("journal.jsonl"));
    }

    #[test]
    fn csv_contains_header_and_rows() {
        let entry = Entry {
            timestamp: "2026-01-01 10:00:00".into(),
            action: "remove".into(),
            target: "firefox".into(),
            ok: true,
            detail: String::new(),
        };
        let csv = to_csv(&[entry]);
        assert!(csv.starts_with("horodatage;action;cible;succes;detail\n"));
        assert!(csv.contains("firefox"));
    }
}
