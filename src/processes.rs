//! Processus et métriques système, lues directement dans `/proc`
//! (aucune dépendance externe, comportement stable d'une version à l'autre).

use crate::disk::{self, Memory};
use crate::exec;
use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant;

/// Un processus en cours d'exécution.
#[derive(Debug, Clone, Serialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub user: String,
    pub cpu: f32,
    pub memory: u64,
    pub state: String,
    pub cmdline: String,
    pub origin: String,
}

/// Métriques globales à un instant donné.
#[derive(Debug, Clone, Copy, Default)]
pub struct Snapshot {
    pub cpu_percent: f32,
    pub process_count: usize,
    pub memory: Memory,
    pub load: (f32, f32, f32),
    pub uptime: u64,
}

/// Champs extraits de `/proc/<pid>/stat`.
#[derive(Debug, Clone)]
pub struct StatFields {
    pub pid: u32,
    pub name: String,
    pub state: String,
    pub ticks: u64,
}

/// Noms de processus à masquer, lus dans `ORBISYS_HIDE_PROCESSES` (liste
/// séparée par des virgules). L'outil de captures s'en sert pour ne pas publier
/// les processus de l'environnement qui produit les images.
fn hidden_process_names() -> Vec<String> {
    std::env::var("ORBISYS_HIDE_PROCESSES")
        .unwrap_or_default()
        .split(',')
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Vrai si `name` fait partie des noms masqués (comparaison insensible à la casse).
fn is_hidden_process(name: &str, hidden: &[String]) -> bool {
    hidden.iter().any(|entry| entry.eq_ignore_ascii_case(name))
}

/// Analyse une ligne de `/proc/<pid>/stat` en tenant compte des parenthèses
/// présentes dans le nom du processus.
pub fn parse_stat(text: &str) -> Option<StatFields> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close <= open {
        return None;
    }
    let pid = text[..open].trim().parse::<u32>().ok()?;
    let name = text[open + 1..close].to_string();
    let rest: Vec<&str> = text[close + 1..].split_whitespace().collect();
    let state = rest.first()?.chars().next()?.to_string();
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    Some(StatFields {
        pid,
        name,
        state,
        ticks: utime + stime,
    })
}

/// Analyse `/proc/meminfo` pour la mémoire d'un processus (kio).
pub fn parse_status_rss(text: &str) -> u64 {
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("VmRSS:") {
            return value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024;
        }
    }
    0
}

/// Analyse `/proc/<pid>/statm` (pages résidentes → octets).
pub fn parse_statm_rss(text: &str) -> u64 {
    let pages: u64 = text
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    pages * 4096
}

/// Secondes CPU consommées / fréquence de l'horloge système.
fn clock_ticks() -> u64 {
    exec::run("getconf", &["CLK_TCK"])
        .ok()
        .and_then(|captured| captured.text().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(100)
}

fn cpu_times() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/stat").unwrap_or_default();
    let line = text.lines().next().unwrap_or("");
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|value| value.parse().ok())
        .collect();
    let total: u64 = values.iter().sum();
    let idle = values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0);
    (total, idle)
}

fn passwd_map() -> HashMap<u32, String> {
    let text = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 3 {
                return None;
            }
            Some((fields[2].parse::<u32>().ok()?, fields[0].to_string()))
        })
        .collect()
}

/// Paquet propriétaire des exécutables (APT), par requête groupée.
fn origin_map(paths: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if !exec::exists("dpkg-query") || paths.is_empty() {
        return map;
    }
    for chunk in paths.chunks(100) {
        let mut args = vec!["-S"];
        args.extend(chunk.iter().map(String::as_str));
        let Ok(captured) = exec::run_with_timeout(
            "dpkg-query",
            &args,
            std::time::Duration::from_secs(8),
        ) else {
            continue;
        };
        for line in captured.stdout.lines() {
            let Some((owners, path)) = line.split_once(": ") else {
                continue;
            };
            let owner = owners.split(',').next().unwrap_or("").trim();
            map.entry(path.trim().to_string())
                .or_insert_with(|| owner.to_string());
        }
    }
    map
}

/// Échantillonneur : conserve l'état précédent pour calculer un usage CPU réel.
pub struct Sampler {
    hz: u64,
    previous_ticks: HashMap<u32, u64>,
    previous_instant: Option<Instant>,
    previous_cpu: Option<(u64, u64)>,
    origins: HashMap<String, String>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            hz: clock_ticks(),
            previous_ticks: HashMap::new(),
            previous_instant: None,
            previous_cpu: None,
            origins: HashMap::new(),
        }
    }

    /// Relit l'ensemble des processus et renvoie les métriques globales.
    pub fn refresh(&mut self) -> (Vec<ProcInfo>, Snapshot) {
        let now = Instant::now();
        let elapsed = self
            .previous_instant
            .map(|previous| now.duration_since(previous).as_secs_f64())
            .unwrap_or(0.0);
        let uptime = disk::uptime_seconds();
        let users = passwd_map();
        let mut current_ticks: HashMap<u32, u64> = HashMap::new();
        let mut entries: Vec<(ProcInfo, u64, String)> = Vec::new();
        let mut exe_paths: Vec<String> = Vec::new();

        let hidden = hidden_process_names();
        if let Ok(dir) = std::fs::read_dir("/proc") {
            for entry in dir.flatten() {
                let file_name = entry.file_name();
                let Some(pid) = file_name.to_str().and_then(|name| name.parse::<u32>().ok()) else {
                    continue;
                };
                let base = format!("/proc/{pid}");
                let Ok(stat_text) = std::fs::read_to_string(format!("{base}/stat")) else {
                    continue;
                };
                let Some(stat) = parse_stat(&stat_text) else {
                    continue;
                };
                if is_hidden_process(&stat.name, &hidden) {
                    continue;
                }
                let memory = std::fs::read_to_string(format!("{base}/statm"))
                    .map(|text| parse_statm_rss(&text))
                    .or_else(|_| {
                        std::fs::read_to_string(format!("{base}/status"))
                            .map(|text| parse_status_rss(&text))
                    })
                    .unwrap_or(0);
                let cmdline = std::fs::read_to_string(format!("{base}/cmdline"))
                    .map(|text| text.replace('\0', " ").trim().to_string())
                    .unwrap_or_default();
                let exe = std::fs::read_link(format!("{base}/exe"))
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default();
                let uid = std::fs::metadata(&base)
                    .ok()
                    .and_then(|meta| {
                        use std::os::unix::fs::MetadataExt;
                        Some(meta.uid())
                    })
                    .unwrap_or(u32::MAX);
                let user = users
                    .get(&uid)
                    .cloned()
                    .unwrap_or_else(|| uid.to_string());
                if !exe.is_empty() {
                    exe_paths.push(exe.clone());
                }
                current_ticks.insert(pid, stat.ticks);
                entries.push((
                    ProcInfo {
                        pid,
                        name: stat.name,
                        user,
                        cpu: 0.0,
                        memory,
                        state: stat.state,
                        cmdline,
                        origin: String::new(),
                    },
                    stat.ticks,
                    exe,
                ));
            }
        }

        exe_paths.sort();
        exe_paths.dedup();
        let unknown: Vec<String> = exe_paths
            .into_iter()
            .filter(|path| !self.origins.contains_key(path))
            .collect();
        if !unknown.is_empty() {
            self.origins.extend(origin_map(&unknown));
        }

        let mut processes = Vec::with_capacity(entries.len());
        for (mut info, ticks, exe) in entries {
            let cpu = match self.previous_ticks.get(&info.pid) {
                Some(previous) if elapsed > 0.0 && ticks >= *previous => {
                    let delta = (ticks - previous) as f64 / self.hz as f64;
                    (delta / elapsed * 100.0) as f32
                }
                _ if uptime > 0 => (ticks as f64 / self.hz as f64 / uptime as f64 * 100.0) as f32,
                _ => 0.0,
            };
            info.cpu = cpu;
            info.origin = self
                .origins
                .get(&exe)
                .cloned()
                .unwrap_or_default();
            processes.push(info);
        }

        processes.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory.cmp(&a.memory))
        });

        self.previous_ticks = current_ticks;
        self.previous_instant = Some(now);

        let cpu_percent = {
            let (total, idle) = cpu_times();
            match self.previous_cpu {
                Some((previous_total, previous_idle))
                    if total > previous_total && idle >= previous_idle =>
                {
                    let total_delta = (total - previous_total) as f32;
                    let idle_delta = (idle - previous_idle) as f32;
                    ((total_delta - idle_delta) / total_delta * 100.0).clamp(0.0, 100.0)
                }
                _ => 0.0,
            }
        };
        self.previous_cpu = Some(cpu_times());

        let snapshot = Snapshot {
            cpu_percent,
            process_count: processes.len(),
            memory: disk::memory(),
            load: disk::load_average(),
            uptime,
        };
        (processes, snapshot)
    }
}

// ---------------------------------------------------------------------------
// Actions sur un processus
// ---------------------------------------------------------------------------

/// Action demandée sur un processus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcAction {
    Terminate,
    Kill,
    Suspend,
    Resume,
    /// Nouvelle priorité (`nice`), de -20 à 19.
    Nice(i32),
}

impl ProcAction {
    pub fn label(self) -> String {
        match self {
            ProcAction::Terminate => "terminer (SIGTERM)".to_string(),
            ProcAction::Kill => "forcer l'arrêt (SIGKILL)".to_string(),
            ProcAction::Suspend => "suspendre (SIGSTOP)".to_string(),
            ProcAction::Resume => "reprendre (SIGCONT)".to_string(),
            ProcAction::Nice(value) => format!("priorité {value}"),
        }
    }
}

/// Nom d'un processus d'après `/proc/<pid>/stat`.
pub fn current_name(pid: u32) -> Option<String> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_stat(&text).map(|fields| fields.name)
}

fn owner_uid(pid: u32) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(format!("/proc/{pid}")).ok().map(|meta| meta.uid())
}

fn own_uid() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").ok().map(|meta| meta.uid())
}

/// Contrôles avant toute action : jamais init, jamais l'application elle-même,
/// jamais un thread du noyau, et le PID doit toujours désigner le processus
/// affiché (un PID peut être réattribué entre l'affichage et le clic).
pub fn check_target(pid: u32, expected_name: &str) -> Result<(), String> {
    if pid <= 1 {
        return Err("refus d'agir sur le processus système (PID ≤ 1)".to_string());
    }
    if pid == std::process::id() {
        return Err("refus d'agir sur l'application elle-même".to_string());
    }
    let Some(name) = current_name(pid) else {
        return Err(format!("le processus {pid} n'existe plus"));
    };
    if name != expected_name {
        return Err(format!(
            "le PID {pid} désigne maintenant « {name} » et non « {expected_name} » : actualisez la liste"
        ));
    }
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    if cmdline.is_empty() {
        return Err("refus d'agir sur un thread du noyau".to_string());
    }
    Ok(())
}

/// Prépare la commande : `(programme, arguments, privilégiée)`.
/// Les processus d'un autre utilisateur, et toute priorité négative, passent
/// par Polkit.
pub fn plan(
    pid: u32,
    expected_name: &str,
    action: ProcAction,
) -> Result<(String, Vec<String>, bool), String> {
    check_target(pid, expected_name)?;
    let foreign = match (owner_uid(pid), own_uid()) {
        (Some(owner), Some(me)) => owner != me,
        _ => true,
    };
    let pid_text = pid.to_string();
    Ok(match action {
        ProcAction::Terminate => ("kill".into(), vec!["-TERM".into(), pid_text], foreign),
        ProcAction::Kill => ("kill".into(), vec!["-KILL".into(), pid_text], foreign),
        ProcAction::Suspend => ("kill".into(), vec!["-STOP".into(), pid_text], foreign),
        ProcAction::Resume => ("kill".into(), vec!["-CONT".into(), pid_text], foreign),
        ProcAction::Nice(value) => {
            if !(-20..=19).contains(&value) {
                return Err(format!("priorité hors limites (-20 à 19) : {value}"));
            }
            (
                "renice".into(),
                vec!["-n".into(), value.to_string(), "-p".into(), pid_text],
                foreign || value < 0,
            )
        }
    })
}

/// Applique l'action après les contrôles de `plan`.
pub fn apply(pid: u32, expected_name: &str, action: ProcAction) -> Result<String, String> {
    let (program, args, privileged) = plan(pid, expected_name, action)?;
    if privileged {
        exec::run_privileged(&program, &args)?;
    } else {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let captured = exec::run(&program, &refs).map_err(|error| error.0)?;
        if !captured.ok {
            return Err(captured.error_text());
        }
    }
    Ok(format!("{expected_name} ({pid}) : {}", action.label()))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stat_with_spaces_in_name() {
        let line = "4242 (mon (drôle de) nom) S 1 4242 4242 0 -1 4194560 100 0 0 0 12 34 0 0 20 0 1 0 500 12345 678 18446744073709551615";
        let stat = parse_stat(line).expect("ligne valide");
        assert_eq!(stat.pid, 4242);
        assert_eq!(stat.name, "mon (drôle de) nom");
        assert_eq!(stat.state, "S");
        assert_eq!(stat.ticks, 46);
    }

    #[test]
    fn parses_memory_fields() {
        assert_eq!(parse_status_rss("Name:\tbash\nVmRSS:\t  2048 kB\n"), 2048 * 1024);
        assert_eq!(parse_statm_rss("100 25 10 5 0 20 0"), 25 * 4096);
    }

    #[test]
    fn rejects_garbage_stat() {
        assert!(parse_stat("pas un stat").is_none());
    }

    #[test]
    fn hides_configured_process_names_case_insensitively() {
        let hidden = vec!["freebuff".to_string()];
        assert!(is_hidden_process("freebuff", &hidden));
        assert!(is_hidden_process("FreeBuff", &hidden));
        assert!(!is_hidden_process("freebuffer", &hidden));
        assert!(!is_hidden_process("freebuff", &[]));
    }

    #[test]
    fn sampler_returns_live_processes() {
        let mut sampler = Sampler::new();
        let (processes, snapshot) = sampler.refresh();
        assert!(!processes.is_empty());
        assert!(snapshot.memory.total > 0);
        assert!(snapshot.process_count >= processes.len());
    }

    fn spawn_sleeper() -> std::process::Child {
        let child = std::process::Command::new("sleep").arg("60").spawn().expect("sleep");
        // Attend la fin du exec : entre fork et exec, le nom et la ligne de commande sont vides.
        for _ in 0..200 {
            if check_target(child.id(), "sleep").is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child
    }

    fn state_of(pid: u32) -> String {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        parse_stat(&text).map(|fields| fields.state).unwrap_or_default()
    }

    #[test]
    fn refuses_dangerous_targets() {
        assert!(check_target(0, "x").is_err());
        assert!(check_target(1, "systemd").is_err());
        assert!(check_target(std::process::id(), "orbisys").is_err());
        assert!(check_target(4_000_000, "x").is_err());
    }

    #[test]
    fn refuses_a_reused_pid() {
        let mut child = spawn_sleeper();
        let pid = child.id();
        let error = check_target(pid, "autre-nom").unwrap_err();
        assert!(error.contains("actualisez"));
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn validates_priority_range() {
        let mut child = spawn_sleeper();
        let pid = child.id();
        assert!(plan(pid, "sleep", ProcAction::Nice(20)).is_err());
        assert!(plan(pid, "sleep", ProcAction::Nice(-21)).is_err());
        let (_, _, privileged) = plan(pid, "sleep", ProcAction::Nice(5)).unwrap();
        assert!(!privileged, "abaisser sa propre priorité ne demande pas de droits");
        let (_, _, privileged) = plan(pid, "sleep", ProcAction::Nice(-5)).unwrap();
        assert!(privileged, "une priorité négative demande Polkit");
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn suspends_resumes_and_terminates_own_process() {
        let mut child = spawn_sleeper();
        let pid = child.id();
        apply(pid, "sleep", ProcAction::Suspend).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(state_of(pid), "T");
        apply(pid, "sleep", ProcAction::Resume).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_ne!(state_of(pid), "T");
        apply(pid, "sleep", ProcAction::Nice(3)).unwrap();
        apply(pid, "sleep", ProcAction::Terminate).unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
    }
}
