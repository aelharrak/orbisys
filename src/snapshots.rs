//! Instantanés et archives : configuration Timeshift, sauvegarde rsync vers
//! un hôte distant et vérification d'archives locales.

use crate::exec;
use crate::util;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// Un instantané Timeshift.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub name: String,
    pub size: u64,
}

/// État de Timeshift.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TimeshiftInfo {
    pub available: bool,
    pub location: String,
    pub schedule: Vec<String>,
    pub keep: String,
    pub snapshots: Vec<Snapshot>,
}

/// État de la page Instantanés & archives.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SnapshotsReport {
    pub timeshift: TimeshiftInfo,
}

/// Analyse `/etc/timeshift/timeshift.json`.
pub fn parse_timeshift_config(text: &str) -> (Vec<String>, String, String) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return (Vec::new(), String::new(), String::new());
    };
    let flag = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(|text| text == "true")
            .unwrap_or(false)
    };
    let mut schedule = Vec::new();
    if flag("schedule_daily") {
        schedule.push("quotidien".to_string());
    }
    if flag("schedule_weekly") {
        schedule.push("hebdomadaire".to_string());
    }
    if flag("schedule_monthly") {
        schedule.push("mensuel".to_string());
    }
    if flag("schedule_boot") {
        schedule.push("au démarrage".to_string());
    }
    let keep = value
        .get("snapshot_count")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let location = value
        .get("backup_device_uuid")
        .and_then(Value::as_str)
        .map(|uuid| format!("périphérique {uuid}"))
        .unwrap_or_default();
    (schedule, keep, location)
}

/// Extrait les noms d'instantanés d'une sortie `timeshift --list`.
pub fn parse_timeshift_list(text: &str) -> Vec<Snapshot> {
    let mut snapshots = Vec::new();
    for line in text.lines() {
        for token in line.split_whitespace() {
            if is_snapshot_name(token) {
                snapshots.push(Snapshot {
                    name: token.to_string(),
                    size: 0,
                });
                break;
            }
        }
    }
    snapshots
}

fn is_snapshot_name(token: &str) -> bool {
    let bytes: Vec<char> = token.chars().collect();
    if bytes.len() != 19 {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| bytes[range].iter().all(|c| c.is_ascii_digit());
    digits(0..4)
        && bytes[4] == '-'
        && digits(5..7)
        && bytes[7] == '-'
        && digits(8..10)
        && bytes[10] == '_'
        && digits(11..13)
        && bytes[13] == '-'
        && digits(14..16)
        && bytes[16] == '-'
        && digits(17..19)
}

/// Dossiers d'instantanés plausibles selon le montage de Timeshift.
pub fn snapshot_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![
        std::path::PathBuf::from("/run/timeshift/backup/timeshift/snapshots"),
        std::path::PathBuf::from("/timeshift/snapshots"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".timeshift/snapshots"));
    }
    dirs
}

/// Une cible distante valide (`user@hôte:/chemin`).
pub fn valid_remote_target(target: &str) -> bool {
    let Some((host_part, path)) = target.split_once(':') else {
        return false;
    };
    if host_part.is_empty() || host_part.len() > 120 || path.is_empty() {
        return false;
    }
    if !path.starts_with('/') || path.contains("..") {
        return false;
    }
    // rsync transmet le chemin à la commande distante : on n'autorise que
    // des caractères sûrs (pas de `;`, `$`, espace, guillemet ni backtick).
    if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '+' | '@'))
    {
        return false;
    }
    let host = host_part.rsplit('@').next().unwrap_or("");
    if host.is_empty() {
        return false;
    }
    host.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && host_part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'))
}

fn remote_host(target: &str) -> String {
    target
        .split_once(':')
        .map(|(host, _)| host.rsplit('@').next().unwrap_or(host).to_string())
        .unwrap_or_default()
}

/// Arguments rsync pour une sauvegarde distante (aucun shell).
pub fn remote_args(source: &str, target: &str, dry_run: bool, checksum: bool) -> Vec<String> {
    let mut args = vec![
        "-a".to_string(),
        "--stats".to_string(),
        "-e".to_string(),
        "ssh -o BatchMode=yes -o ConnectTimeout=10".to_string(),
    ];
    if checksum {
        args.push("--checksum".to_string());
    }
    if dry_run {
        args.push("--dry-run".to_string());
    }
    args.push(source.to_string());
    args.push(target.to_string());
    args
}

/// Teste la connexion SSH vers la cible (sans mot de passe interactif).
pub fn check_remote(target: &str) -> Result<String, String> {
    if !valid_remote_target(target) {
        return Err("cible invalide : format attendu user@hôte:/chemin".to_string());
    }
    let host = remote_host(target);
    if host.is_empty() {
        return Err("hôte introuvable dans la cible".to_string());
    }
    let captured = exec::run(
        "ssh",
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            &host,
            "true",
        ],
    )
    .map_err(|error| error.0)?;
    if captured.ok {
        Ok(format!("connexion à {host} établie"))
    } else {
        Err(format!(
            "connexion à {host} impossible : {}",
            captured.error_text()
        ))
    }
}

/// Lance une sauvegarde rsync distante (simulation ou réelle).
pub fn remote_backup(source: &str, target: &str, dry_run: bool) -> Result<String, String> {
    if !valid_remote_target(target) {
        return Err("cible invalide : format attendu user@hôte:/chemin".to_string());
    }
    if !Path::new(source).exists() {
        return Err(format!("source introuvable : {source}"));
    }
    if source.contains('\n') || target.contains('\n') {
        return Err("chemin refusé".to_string());
    }
    let args = remote_args(source, target, dry_run, false);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let captured = exec::run_with_timeout("rsync", &refs, std::time::Duration::from_secs(3_600))
        .map_err(|error| error.0)?;
    if captured.ok {
        Ok(captured.text())
    } else {
        Err(captured.error_text())
    }
}

/// Vérifie une archive locale sans la décompresser sur disque.
pub fn verify_archive(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err(format!("fichier introuvable : {}", path.display()));
    }
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let display = path.display().to_string();
    let captured = if name.ends_with(".zip") {
        exec::run("unzip", &["-t", &display])
    } else if name.ends_with(".tar")
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".tar.xz")
        || name.ends_with(".txz")
        || name.ends_with(".tar.bz2")
        || name.ends_with(".tbz2")
    {
        exec::run("tar", &["-tf", &display])
    } else {
        return Err("format non pris en charge : attendu .tar[.gz|.xz|.bz2], .tgz, .txz, .zip".to_string());
    }
    .map_err(|error| error.0)?;
    if !captured.ok {
        return Err(format!("archive invalide : {}", captured.error_text()));
    }
    let entries = captured.stdout.lines().filter(|line| !line.trim().is_empty()).count();
    Ok(format!(
        "archive valide : {entries} entrée(s) · {}",
        util::human_bytes(std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0))
    ))
}

/// Lit l'état de Timeshift et des instantanés.
pub fn report() -> SnapshotsReport {
    let available = exec::exists("timeshift");
    let mut info = TimeshiftInfo {
        available,
        ..Default::default()
    };
    if let Ok(text) = std::fs::read_to_string("/etc/timeshift/timeshift.json") {
        let (schedule, keep, location) = parse_timeshift_config(&text);
        info.schedule = schedule;
        info.keep = keep;
        info.location = location;
    }
    for dir in snapshot_dirs() {
        if !dir.is_dir() {
            continue;
        }
        info.location = dir.display().to_string();
        let mut snapshots: Vec<Snapshot> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir())
                    .map(|entry| {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if is_snapshot_name(&name) {
                            Snapshot {
                                name,
                                size: util::dir_size(&entry.path(), 20_000),
                            }
                        } else {
                            Snapshot {
                                name,
                                size: 0,
                            }
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        snapshots.sort_by(|a, b| b.name.cmp(&a.name));
        info.snapshots = snapshots;
        break;
    }
    SnapshotsReport { timeshift: info }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timeshift_config() {
        let text = r#"{"backup_device_uuid":"abcd-1234","schedule_daily":"true","schedule_weekly":"false","schedule_monthly":"true","schedule_boot":"false","snapshot_count":"5"}"#;
        let (schedule, keep, location) = parse_timeshift_config(text);
        assert_eq!(schedule, vec!["quotidien", "mensuel"]);
        assert_eq!(keep, "5");
        assert!(location.contains("abcd-1234"));
    }

    #[test]
    fn recognises_snapshot_names() {
        assert!(is_snapshot_name("2026-11-01_10-00-00"));
        assert!(!is_snapshot_name("2026-11-01"));
        assert!(!is_snapshot_name("snapshot-2026"));
        assert!(!is_snapshot_name("2026-11-01_10-00-0"));
    }

    #[test]
    fn parses_timeshift_listing() {
        let text = "Num     Name                 Tags  Description\n------------------------------------------------------------------------------\n0    >  2026-11-01_10-00-00  O     Avant mise à jour\n1    >  2026-10-25_09-30-00  O\n";
        let snapshots = parse_timeshift_list(text);
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].name, "2026-11-01_10-00-00");
    }

    #[test]
    fn validates_remote_targets() {
        assert!(valid_remote_target("amine@192.168.1.20:/srv/sauvegarde"));
        assert!(valid_remote_target("backup:/mnt/archives"));
        // rsync accepte `hôte:/chemin` sans utilisateur explicite.
        assert!(valid_remote_target("192.168.1.20:/srv"));
        assert!(!valid_remote_target("amine@srv:relative/path"));
        assert!(!valid_remote_target("amine@srv:/srv/../../etc"));
        assert!(!valid_remote_target("amine@srv:/srv;reboot"));
    }

    #[test]
    fn builds_rsync_remote_arguments() {
        let args = remote_args("/home/amine", "amine@srv:/srv/backup", true, false);
        assert!(args.contains(&"--dry-run".to_string()));
        assert!(args.contains(&"ssh -o BatchMode=yes -o ConnectTimeout=10".to_string()));
        assert_eq!(args.last().unwrap(), "amine@srv:/srv/backup");
    }

    #[test]
    fn extracts_remote_host() {
        assert_eq!(remote_host("amine@srv:/srv/backup"), "srv");
        assert_eq!(remote_host("srv:/srv/backup"), "srv");
    }

    #[test]
    fn rejects_unsupported_archives() {
        let path = std::env::temp_dir().join(format!("um-note-{}.txt", std::process::id()));
        std::fs::write(&path, b"pas une archive").unwrap();
        assert!(verify_archive(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
