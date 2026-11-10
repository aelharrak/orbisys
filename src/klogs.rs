//! Journaux du noyau : dmesg/journalctl -k, classification des erreurs,
//! OOM, erreurs matérielles et liste des démarrages.

use crate::exec;
use serde::Serialize;

/// Niveau d'une ligne du noyau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Level {
    Error,
    Warning,
    Info,
}

impl Level {
    pub fn label(&self) -> &'static str {
        match self {
            Level::Error => "erreur",
            Level::Warning => "avertissement",
            Level::Info => "information",
        }
    }
}

/// Catégorie d'un événement noyau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Category {
    OutOfMemory,
    Hardware,
    Storage,
    Thermal,
    Network,
    Graphics,
    Other,
}

impl Category {
    pub fn label(&self) -> &'static str {
        match self {
            Category::OutOfMemory => "mémoire",
            Category::Hardware => "matériel",
            Category::Storage => "stockage",
            Category::Thermal => "thermique",
            Category::Network => "réseau",
            Category::Graphics => "graphique",
            Category::Other => "divers",
        }
    }
}

/// Un événement noyau classifié.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub line: String,
    pub level: Level,
    pub category: Category,
}

impl Event {
    /// Une ligne qui mérite l'attention.
    pub fn notable(&self) -> bool {
        self.level != Level::Info
    }
}

/// Un démarrage listé par `journalctl --list-boots`.
#[derive(Debug, Clone, Serialize)]
pub struct Boot {
    pub index: i64,
    pub id: String,
    pub range: String,
}

/// Rapport des journaux noyau.
#[derive(Debug, Clone, Default)]
pub struct KernelReport {
    pub version: String,
    pub boots: Vec<Boot>,
    pub events: Vec<Event>,
    pub errors: Vec<String>,
}

impl KernelReport {
    pub fn count(&self, level: Level) -> usize {
        self.events.iter().filter(|event| event.level == level).count()
    }

    pub fn count_category(&self, category: Category) -> usize {
        self.events
            .iter()
            .filter(|event| event.category == category)
            .count()
    }

    /// Événements notables, les plus récents d'abord.
    pub fn notable(&self) -> Vec<&Event> {
        self.events.iter().filter(|event| event.notable()).rev().collect()
    }
}

/// Classe une ligne du noyau par niveau et par domaine.
pub fn classify(line: &str) -> (Level, Category) {
    let lower = line.to_ascii_lowercase();
    let error = lower.contains("error")
        || lower.contains("failed")
        || lower.contains("failure")
        || lower.contains("panic")
        || lower.contains(" oops")
        || lower.contains("bug:")
        || lower.contains("timeout")
        || lower.contains("reset");
    let warning = lower.contains("warn") || lower.contains("throttl") || lower.contains("deprecated");

    let category = if lower.contains("out of memory")
        || lower.contains("oom-kill")
        || lower.contains("killed process")
    {
        Category::OutOfMemory
    } else if lower.contains("hardware error")
        || lower.contains("machine check")
        || lower.contains("mce:")
        || lower.contains("corrected error")
    {
        Category::Hardware
    } else if lower.contains("i/o error")
        || lower.contains("buffer i/o")
        || lower.contains("medium error")
        || (lower.contains("ata") && lower.contains("failed"))
        || (lower.contains("nvme") && (lower.contains("timeout") || lower.contains("reset")))
    {
        Category::Storage
    } else if lower.contains("thermal") || lower.contains("temperature above") {
        Category::Thermal
    } else if lower.contains("link is down")
        || lower.contains("carrier lost")
        || lower.contains("firmware failed")
        || lower.contains("tx timeout")
    {
        Category::Network
    } else if lower.contains("gpu hang")
        || lower.contains("amdgpu")
        || lower.contains("i915")
        || lower.contains("drm:")
        || lower.contains("nouveau")
    {
        Category::Graphics
    } else {
        Category::Other
    };

    // Certaines catégories sont fautives par nature : « Out of memory: Killed
    // process » ne contient aucun mot-clé d'erreur explicite.
    let level = match category {
        Category::OutOfMemory | Category::Hardware | Category::Storage => Level::Error,
        Category::Thermal | Category::Network | Category::Graphics => {
            if lower.contains("panic") || error {
                Level::Error
            } else {
                Level::Warning
            }
        }
        Category::Other => {
            if lower.contains("panic") || error {
                Level::Error
            } else if warning {
                Level::Warning
            } else {
                Level::Info
            }
        }
    };
    (level, category)
}

/// Analyse une sortie `dmesg`/`journalctl -k`.
pub fn parse_output(text: &str) -> Vec<Event> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (level, category) = classify(line);
            Event {
                line: line.trim_end().to_string(),
                level,
                category,
            }
        })
        .collect()
}

/// Analyse `journalctl --list-boots`.
pub fn parse_boots(text: &str) -> Vec<Boot> {
    let mut boots: Vec<Boot> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let mut fields = trimmed.splitn(3, char::is_whitespace);
        let Some(index) = fields.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let id = fields.next().unwrap_or("").to_string();
        let range = fields.next().unwrap_or("").trim().to_string();
        boots.push(Boot { index, id, range });
    }
    boots.sort_by(|a, b| b.index.cmp(&a.index));
    boots
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

/// Collecte les journaux noyau.
pub fn report() -> KernelReport {
    let mut report = KernelReport {
        version: read("/proc/sys/kernel/osrelease").unwrap_or_else(|| "inconnu".to_string()),
        ..KernelReport::default()
    };
    if let Ok(captured) = exec::run_with_timeout(
        "journalctl",
        &["-k", "-o", "short-iso", "-n", "4000"],
        std::time::Duration::from_secs(60),
    ) {
        if captured.ok || !captured.stdout.trim().is_empty() {
            report.events = parse_output(&captured.stdout);
        } else {
            report.errors.push(captured.error_text());
        }
    }
    if report.events.is_empty() {
        match exec::run_with_timeout("dmesg", &[], std::time::Duration::from_secs(30)) {
            Ok(captured) if captured.ok => report.events = parse_output(&captured.stdout),
            Ok(captured) => report.errors.push(captured.error_text()),
            Err(error) => report.errors.push(error.0),
        }
    }
    if let Ok(captured) = exec::run(
        "journalctl",
        &["--list-boots", "--no-pager"],
    ) {
        report.boots = parse_boots(&captured.stdout);
    }
    report.events.truncate(4000);
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_oom_and_hardware() {
        let (level, category) = classify("Out of memory: Killed process 4242 (firefox)");
        assert_eq!(level, Level::Error);
        assert_eq!(category, Category::OutOfMemory);
        let (level, category) = classify("[Hardware Error]: Corrected error, no action required.");
        assert_eq!(level, Level::Error);
        assert_eq!(category, Category::Hardware);
    }

    #[test]
    fn classifies_storage_and_warnings() {
        let (_, category) = classify("ata1.00: failed command: READ FPDMA QUEUED");
        assert_eq!(category, Category::Storage);
        let (level, category) = classify("thermal thermal_zone0: temperature above threshold");
        assert_eq!(level, Level::Warning);
        assert_eq!(category, Category::Thermal);
    }

    #[test]
    fn keeps_info_lines_quiet() {
        let (level, category) = classify("Linux version 6.8.0-45-generic (buildd@lcy02)");
        assert_eq!(level, Level::Info);
        assert_eq!(category, Category::Other);
        assert!(!Event {
            line: String::new(),
            level,
            category,
        }
        .notable());
    }

    #[test]
    fn parses_boot_listing() {
        let text = " 0 4f3c2a1b Tue 2026-11-02 09:00:00 CET—Tue 2026-11-02 18:00:00 CET\n-1 9a8b7c6d Mon 2026-11-01 08:00:00 CET—Mon 2026-11-01 22:00:00 CET\n";
        let boots = parse_boots(text);
        assert_eq!(boots.len(), 2);
        assert_eq!(boots[0].index, 0);
        assert_eq!(boots[0].id, "4f3c2a1b");
        assert!(boots[0].range.contains("2026-11-02"));
        assert_eq!(boots[1].index, -1);
    }

    #[test]
    fn counts_events_by_level_and_category() {
        let text = "[    1.0] Linux version 6.8\n[    2.0] Out of memory: Killed process 10\n[    3.0] ata1: failed command\n";
        let report = KernelReport {
            events: parse_output(text),
            ..KernelReport::default()
        };
        assert_eq!(report.events.len(), 3);
        assert_eq!(report.count(Level::Error), 2);
        assert_eq!(report.count(Level::Info), 1);
        assert_eq!(report.count_category(Category::OutOfMemory), 1);
        assert_eq!(report.notable().len(), 2);
    }
}
