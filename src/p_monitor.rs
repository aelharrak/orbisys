//! Supervision : surveillance en temps réel, alertes, santé, historique,
//! benchmark et livraison des rapports.

use crate::disk;
use crate::hardware;
use crate::panel::*;
use crate::util;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Historique en mémoire de la surveillance en direct
// ---------------------------------------------------------------------------

/// Nombre d'échantillons conservés (3 s d'intervalle : environ 6 minutes).
pub const LIVE_CAPACITY: usize = 120;

/// Un échantillon de la surveillance : processeur, mémoire (%), réception et émission (octets/s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveSample {
    pub cpu: f32,
    pub mem: f32,
    pub rx: f32,
    pub tx: f32,
}

static LIVE: std::sync::Mutex<std::collections::VecDeque<LiveSample>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());

/// Ajoute un échantillon dans un anneau borné.
pub fn push_bounded(ring: &mut std::collections::VecDeque<LiveSample>, sample: LiveSample, capacity: usize) {
    ring.push_back(sample);
    while ring.len() > capacity {
        ring.pop_front();
    }
}

fn live_push(sample: LiveSample) -> Vec<LiveSample> {
    let mut ring = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    push_bounded(&mut ring, sample, LIVE_CAPACITY);
    ring.iter().copied().collect()
}

/// Graphiques de la surveillance à partir de l'historique.
pub fn live_charts(history: &[LiveSample]) -> Vec<Chart> {
    let column = |pick: fn(&LiveSample) -> f32| -> Vec<f32> { history.iter().map(pick).collect() };
    vec![
        Chart::lines(
            "Processeur et mémoire",
            "%",
            vec![Series::new("Processeur", column(|s| s.cpu)), Series::new("Mémoire", column(|s| s.mem))],
        )
        .max_y(100.0),
        Chart::lines(
            "Débit réseau",
            "B/s",
            vec![Series::new("Réception", column(|s| s.rx)), Series::new("Émission", column(|s| s.tx))],
        ),
    ]
}

// ---------------------------------------------------------------------------
// Échantillonnage /proc
// ---------------------------------------------------------------------------

/// `(inactif, total)` de la ligne `cpu` de `/proc/stat`.
pub fn parse_proc_stat(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|line| line.starts_with("cpu "))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|value| value.parse().ok())
        .collect();
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some((idle, values.iter().sum()))
}

/// Pourcentage d'utilisation entre deux échantillons.
pub fn cpu_percent(before: (u64, u64), after: (u64, u64)) -> f32 {
    let total = after.1.saturating_sub(before.1);
    let idle = after.0.saturating_sub(before.0);
    if total == 0 {
        0.0
    } else {
        (total.saturating_sub(idle)) as f32 * 100.0 / total as f32
    }
}

/// `(interface, octets reçus, octets émis)` de `/proc/net/dev`.
pub fn parse_net_dev(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .skip(2)
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let fields: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect();
            if fields.len() < 9 {
                return None;
            }
            Some((name.trim().to_string(), fields[0], fields[8]))
        })
        .collect()
}

/// Disque entier (pas une partition ni un périphérique virtuel).
pub fn is_whole_disk(name: &str) -> bool {
    if name.starts_with("sd") || name.starts_with("vd") || name.starts_with("hd") || name.starts_with("xvd") {
        return !name.ends_with(|ch: char| ch.is_ascii_digit());
    }
    if let Some(rest) = name.strip_prefix("nvme") {
        return !rest.contains('p');
    }
    if let Some(rest) = name.strip_prefix("mmcblk") {
        return !rest.contains('p');
    }
    false
}

/// `(périphérique, secteurs lus, secteurs écrits)` de `/proc/diskstats`,
/// limité aux disques entiers.
pub fn parse_diskstats(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 {
                return None;
            }
            let name = fields[2];
            let whole = is_whole_disk(name);
            if !whole {
                return None;
            }
            Some((name.to_string(), fields[5].parse().ok()?, fields[9].parse().ok()?))
        })
        .collect()
}

fn rate(before: u64, after: u64, seconds: f32) -> u64 {
    if seconds <= 0.0 {
        0
    } else {
        (after.saturating_sub(before) as f32 / seconds) as u64
    }
}

/// Utilisation du processeur mesurée sur `window`.
pub fn sample_cpu(window: Duration) -> f32 {
    let before = parse_proc_stat(&read("/proc/stat"));
    std::thread::sleep(window);
    let after = parse_proc_stat(&read("/proc/stat"));
    match (before, after) {
        (Some(before), Some(after)) => cpu_percent(before, after),
        _ => 0.0,
    }
}

/// Utilisation maximale des systèmes de fichiers montés.
pub fn disk_percent() -> f32 {
    disk::mounts()
        .iter()
        .map(|mount| mount.percent)
        .fold(0.0, f32::max)
}

/// Température maximale lue.
pub fn max_temperature() -> Option<f32> {
    hardware::temperatures()
        .iter()
        .map(|(_, value)| *value)
        .fold(None, |best: Option<f32>, value| Some(best.map_or(value, |b| b.max(value))))
}

/// Surveillance en temps réel.
pub fn monitor() -> Panel {
    let mut panel = Panel::default();
    let started = Instant::now();
    let cpu_before = parse_proc_stat(&read("/proc/stat"));
    let net_before = parse_net_dev(&read("/proc/net/dev"));
    let disks_before = parse_diskstats(&read("/proc/diskstats"));
    std::thread::sleep(Duration::from_millis(500));
    let cpu_after = parse_proc_stat(&read("/proc/stat"));
    let net_after = parse_net_dev(&read("/proc/net/dev"));
    let disks_after = parse_diskstats(&read("/proc/diskstats"));
    let seconds = started.elapsed().as_secs_f32();

    let cpu = match (cpu_before, cpu_after) {
        (Some(before), Some(after)) => cpu_percent(before, after),
        _ => 0.0,
    };
    let memory = disk::memory();
    let (one, five, fifteen) = disk::load_average();
    panel.metric("Processeur", format!("{cpu:.0} %"), level_for_percent(cpu, 70.0, 90.0));
    panel.metric(
        "Mémoire",
        format!(
            "{:.0} % ({} / {})",
            memory.percent(),
            util::human_bytes(memory.used()),
            util::human_bytes(memory.total)
        ),
        level_for_percent(memory.percent(), 75.0, 92.0),
    );
    panel.metric(
        "Échange",
        format!("{} / {}", util::human_bytes(memory.swap_used()), util::human_bytes(memory.swap_total)),
        Level::Info,
    );
    panel.metric("Charge", format!("{one:.2} {five:.2} {fifteen:.2}"), Level::Info);
    panel.metric("Allumé depuis", util::human_duration(disk::uptime_seconds()), Level::Info);

    let (mut total_rx, mut total_tx) = (0.0f32, 0.0f32);
    let mut rows = Vec::new();
    for (name, rx_after, tx_after) in &net_after {
        if name == "lo" {
            continue;
        }
        let (rx_before, tx_before) = net_before
            .iter()
            .find(|(other, _, _)| other == name)
            .map(|(_, rx, tx)| (*rx, *tx))
            .unwrap_or((*rx_after, *tx_after));
        total_rx += rate(rx_before, *rx_after, seconds) as f32;
        total_tx += rate(tx_before, *tx_after, seconds) as f32;
        rows.push(Row::owned(vec![
            name.clone(),
            format!("{}/s", util::human_bytes(rate(rx_before, *rx_after, seconds))),
            format!("{}/s", util::human_bytes(rate(tx_before, *tx_after, seconds))),
            util::human_bytes(*rx_after),
            util::human_bytes(*tx_after),
        ]));
    }
    let history = live_push(LiveSample { cpu, mem: memory.percent(), rx: total_rx, tx: total_tx });
    let mut charts = live_charts(&history).into_iter();
    let mut graphs = Section::new("Courbes en direct")
        .note("Les 6 dernières minutes environ, actualisées toutes les 3 secondes.");
    for chart in charts.by_ref() {
        graphs = graphs.chart(chart);
    }
    panel.section(graphs);
    panel.section(
        Section::new("Réseau")
            .columns(&["Interface", "Réception", "Émission", "Total reçu", "Total émis"])
            .rows(rows),
    );

    let mut rows = Vec::new();
    for (name, read_after, write_after) in &disks_after {
        let (read_before, write_before) = disks_before
            .iter()
            .find(|(other, _, _)| other == name)
            .map(|(_, read, write)| (*read, *write))
            .unwrap_or((*read_after, *write_after));
        rows.push(Row::owned(vec![
            name.clone(),
            format!("{}/s", util::human_bytes(rate(read_before, *read_after, seconds) * 512)),
            format!("{}/s", util::human_bytes(rate(write_before, *write_after, seconds) * 512)),
        ]));
    }
    panel.section(
        Section::new("Disques")
            .columns(&["Disque", "Lecture", "Écriture"])
            .rows(rows)
            .note("Lecture seule : aucune donnée du système n'est modifiée."),
    );
    panel
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Alertes
// ---------------------------------------------------------------------------

/// Seuils d'alerte (page « Alertes »).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Thresholds {
    pub cpu: f32,
    pub mem: f32,
    pub disk: f32,
    pub temp: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            cpu: 90.0,
            mem: 92.0,
            disk: 90.0,
            temp: 80.0,
        }
    }
}

const THRESHOLDS_FILE: &str = "alerts.json";

pub fn thresholds() -> Thresholds {
    load_json(THRESHOLDS_FILE).unwrap_or_default()
}

/// Valide quatre seuils saisis (cpu, mem, disque : 1-100 ; température : 1-150).
pub fn parse_thresholds(values: &[String]) -> Result<Thresholds, String> {
    if values.len() != 4 {
        return Err("quatre seuils attendus".to_string());
    }
    let mut parsed = [0.0f32; 4];
    for (index, text) in values.iter().enumerate() {
        let value: f32 = text
            .trim()
            .parse()
            .map_err(|_| format!("valeur invalide : {text}"))?;
        let max = if index == 3 { 150.0 } else { 100.0 };
        if !(1.0..=max).contains(&value) {
            return Err(format!("seuil hors limites (1-{max}) : {value}"));
        }
        parsed[index] = value;
    }
    Ok(Thresholds {
        cpu: parsed[0],
        mem: parsed[1],
        disk: parsed[2],
        temp: parsed[3],
    })
}

fn save_thresholds(values: &[String]) -> Result<String, String> {
    let thresholds = parse_thresholds(values)?;
    save_json(THRESHOLDS_FILE, &thresholds)?;
    Ok("Seuils enregistrés".to_string())
}

fn build_thresholds(values: &[String]) -> Result<Cmd, String> {
    parse_thresholds(values)?;
    Ok(Cmd::local("Seuils", save_thresholds, values.to_vec()))
}

/// Liste des dépassements : `(libellé, valeur, seuil)`.
pub fn breaches(current: &Measure, limits: &Thresholds) -> Vec<(&'static str, f32, f32)> {
    let mut found = Vec::new();
    if current.cpu >= limits.cpu {
        found.push(("Processeur", current.cpu, limits.cpu));
    }
    if current.mem >= limits.mem {
        found.push(("Mémoire", current.mem, limits.mem));
    }
    if current.disk >= limits.disk {
        found.push(("Disque", current.disk, limits.disk));
    }
    if let Some(temp) = current.temp {
        if temp >= limits.temp {
            found.push(("Température", temp, limits.temp));
        }
    }
    found
}

/// Une mesure instantanée.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Measure {
    pub cpu: f32,
    pub mem: f32,
    pub disk: f32,
    pub temp: Option<f32>,
}

pub fn measure_now() -> Measure {
    Measure {
        cpu: sample_cpu(Duration::from_millis(400)),
        mem: disk::memory().percent(),
        disk: disk_percent(),
        temp: max_temperature(),
    }
}

/// Dépassements actuels sous forme de texte : `(indicateur, message)`.
pub fn breaches_now() -> Vec<(String, String)> {
    let limits = thresholds();
    let current = measure_now();
    // Chaque contrôle alimente aussi l'historique des tendances.
    let _ = append_sample(&current);
    breaches(&current, &limits)
        .into_iter()
        .map(|(label, value, limit)| {
            (
                label.to_string(),
                format!("{label} à {value:.0} (seuil {limit:.0})"),
            )
        })
        .collect()
}

pub fn alerts() -> Panel {
    let mut panel = Panel::default();
    let limits = thresholds();
    let current = measure_now();
    let found = breaches(&current, &limits);
    panel.metric(
        "Alertes actives",
        found.len().to_string(),
        if found.is_empty() { Level::Ok } else { Level::Bad },
    );
    let rows = [
        ("Processeur (%)", current.cpu, limits.cpu),
        ("Mémoire (%)", current.mem, limits.mem),
        ("Disque (%)", current.disk, limits.disk),
    ]
    .iter()
    .map(|(label, value, limit)| {
        Row::owned(vec![
            label.to_string(),
            format!("{value:.0}"),
            format!("{limit:.0}"),
            if value >= limit { "dépassé" } else { "normal" }.to_string(),
        ])
        .bad(value >= limit)
    })
    .chain(std::iter::once({
        let (shown, over) = match current.temp {
            Some(temp) => (format!("{temp:.0}"), temp >= limits.temp),
            None => ("—".to_string(), false),
        };
        Row::owned(vec![
            "Température (°C)".to_string(),
            shown,
            format!("{:.0}", limits.temp),
            if over { "dépassé" } else { "normal" }.to_string(),
        ])
        .bad(over)
    }))
    .collect();
    let mut labels = vec!["Processeur".to_string(), "Mémoire".to_string(), "Disque".to_string()];
    let mut values = vec![current.cpu, current.mem, current.disk];
    let mut marks = vec![limits.cpu, limits.mem, limits.disk];
    if let Some(temp) = current.temp {
        labels.push("Température".to_string());
        values.push(temp);
        marks.push(limits.temp);
    }
    let mut section = Section::new("Seuils")
        .chart(Chart::bars("Valeur actuelle et seuil (trait)", "", labels, values).max_y(100.0).marks(marks))
        .columns(&["Indicateur", "Valeur", "Seuil", "État"])
        .rows(rows)
        .note("Les seuils sont conservés dans ~/.local/share/orbisys/alerts.json.");
    if !found.is_empty() {
        let body = found
            .iter()
            .map(|(label, value, limit)| format!("{label} {value:.0} (seuil {limit:.0})"))
            .collect::<Vec<_>>()
            .join(", ");
        section = section.action(Cmd::new(
            "Notifier maintenant",
            "notify-send",
            &["OrbiSys", &body],
        ));
    }
    section = section
        .action(Cmd::new(
            "Tester la notification",
            "notify-send",
            &["OrbiSys", "Notification de test"],
        ))
        .form(Form {
            fields: vec![
                Field::new("Processeur", "%").with_default(&format!("{:.0}", limits.cpu)),
                Field::new("Mémoire", "%").with_default(&format!("{:.0}", limits.mem)),
                Field::new("Disque", "%").with_default(&format!("{:.0}", limits.disk)),
                Field::new("Température", "°C").with_default(&format!("{:.0}", limits.temp)),
            ],
            submit: "Enregistrer les seuils".to_string(),
            build: build_thresholds,
        });
    panel.section(section);
    panel
}

// ---------------------------------------------------------------------------
// Historique
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub timestamp: String,
    pub cpu: f32,
    pub mem: f32,
    pub disk: f32,
}

fn history_path() -> std::path::PathBuf {
    data_dir().join("history.jsonl")
}

pub fn history_samples(limit: usize) -> Vec<Sample> {
    let text = std::fs::read_to_string(history_path()).unwrap_or_default();
    let mut samples: Vec<Sample> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if samples.len() > limit {
        samples.drain(..samples.len() - limit);
    }
    samples
}

/// Courbe en blocs Unicode sur une échelle de 0 à 100.
pub fn sparkline(values: &[f32]) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    values
        .iter()
        .map(|value| {
            let index = ((value.clamp(0.0, 100.0) / 100.0) * 7.0).round() as usize;
            BLOCKS[index.min(7)]
        })
        .collect()
}

/// Taille au-delà de laquelle l'historique est réduit à sa moitié la plus récente.
const HISTORY_MAX_BYTES: u64 = 600_000;

/// Ajoute une mesure à l'historique persistant.
pub fn append_sample(current: &Measure) -> Result<(), String> {
    let sample = Sample {
        timestamp: util::now_human(),
        cpu: current.cpu,
        mem: current.mem,
        disk: current.disk,
    };
    std::fs::create_dir_all(data_dir()).map_err(|error| error.to_string())?;
    let path = history_path();
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > HISTORY_MAX_BYTES) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let _ = std::fs::write(&path, lines[lines.len() / 2..].join("\n") + "\n");
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    writeln!(file, "{}", serde_json::to_string(&sample).map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())
}

fn record_sample(_: &[String]) -> Result<String, String> {
    append_sample(&measure_now())?;
    Ok("Mesure enregistrée".to_string())
}

fn clear_history(_: &[String]) -> Result<String, String> {
    match std::fs::remove_file(history_path()) {
        Ok(()) => Ok("Historique effacé".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("Historique vide".to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn samples_to_csv(samples: &[Sample]) -> String {
    let mut out = String::from("horodatage;cpu;memoire;disque\n");
    for sample in samples {
        out.push_str(&format!(
            "{};{:.1};{:.1};{:.1}\n",
            sample.timestamp, sample.cpu, sample.mem, sample.disk
        ));
    }
    out
}

fn export_history(_: &[String]) -> Result<String, String> {
    let path = home().join("orbisys-historique.csv");
    std::fs::write(&path, samples_to_csv(&history_samples(100_000)))
        .map_err(|error| error.to_string())?;
    Ok(format!("Exporté : {}", path.display()))
}

pub fn history() -> Panel {
    let mut panel = Panel::default();
    let samples = history_samples(500);
    let average = |pick: fn(&Sample) -> f32| -> f32 {
        if samples.is_empty() {
            0.0
        } else {
            samples.iter().map(pick).sum::<f32>() / samples.len() as f32
        }
    };
    panel.metric("Mesures", samples.len().to_string(), Level::Info);
    panel.metric("CPU moyen", format!("{:.0} %", average(|s| s.cpu)), Level::Info);
    panel.metric("Mémoire moyenne", format!("{:.0} %", average(|s| s.mem)), Level::Info);
    panel.metric("Disque moyen", format!("{:.0} %", average(|s| s.disk)), Level::Info);
    let last: Vec<&Sample> = samples.iter().rev().take(40).rev().collect();
    let series = |label: &str, pick: fn(&Sample) -> f32| Series::new(label, last.iter().map(|s| pick(s)).collect());
    panel.section(
        Section::new("Courbes")
            .action(Cmd::local("Enregistrer une mesure", record_sample, Vec::new()))
            .action(Cmd::local("Exporter en CSV", export_history, Vec::new()))
            .action(
                Cmd::local("Effacer l'historique", clear_history, Vec::new())
                    .confirm("Supprimer toutes les mesures enregistrées ?"),
            )
            .chart(
                Chart::lines(
                    "Processeur, mémoire et disque",
                    "%",
                    vec![series("Processeur", |s| s.cpu), series("Mémoire", |s| s.mem), series("Disque", |s| s.disk)],
                )
                .max_y(100.0),
            ),
    );
    let rows = samples
        .iter()
        .rev()
        .take(30)
        .map(|s| {
            Row::owned(vec![
                s.timestamp.clone(),
                format!("{:.0} %", s.cpu),
                format!("{:.0} %", s.mem),
                format!("{:.0} %", s.disk),
            ])
        })
        .collect();
    panel.section(
        Section::new("Dernières mesures")
            .columns(&["Date", "CPU", "Mémoire", "Disque"])
            .rows(rows),
    );
    panel
}

// ---------------------------------------------------------------------------
// Santé du système
// ---------------------------------------------------------------------------

/// Un contrôle de santé.
#[derive(Debug, Clone)]
pub struct Check {
    pub label: String,
    pub ok: Option<bool>,
    pub detail: String,
    pub penalty: u32,
}

/// Score sur 100 : chaque contrôle en échec retire sa pénalité.
pub fn health_score(checks: &[Check]) -> u32 {
    let lost: u32 = checks
        .iter()
        .filter(|check| check.ok == Some(false))
        .map(|check| check.penalty)
        .sum();
    100u32.saturating_sub(lost)
}

/// Nombre d'unités systemd en échec d'après `systemctl --failed`.
pub fn count_failed_units(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start_matches(['●', ' ', '*']);
            trimmed.split_whitespace().next().is_some_and(|name| {
                name.contains('.')
                    && (name.ends_with(".service")
                        || name.ends_with(".mount")
                        || name.ends_with(".timer")
                        || name.ends_with(".socket"))
            })
        })
        .count()
}

/// État d'UFW d'après `ufw status` (français ou anglais) ; `None` si illisible.
pub fn parse_ufw_state(text: &str) -> Option<bool> {
    let lower = text.to_lowercase();
    let line = lower
        .lines()
        .find(|line| line.starts_with("status") || line.starts_with("statut") || line.starts_with("état") || line.starts_with("etat"))?;
    if line.contains("inactive") || line.contains("inactif") {
        Some(false)
    } else if line.contains("active") || line.contains("actif") {
        Some(true)
    } else {
        None
    }
}

pub fn health_checks() -> Vec<Check> {
    let mut checks = Vec::new();
    let disk = disk_percent();
    checks.push(Check {
        label: "Espace disque".to_string(),
        ok: Some(disk < 90.0),
        detail: format!("occupation maximale {disk:.0} %"),
        penalty: 20,
    });
    let memory = disk::memory().percent();
    checks.push(Check {
        label: "Mémoire".to_string(),
        ok: Some(memory < 92.0),
        detail: format!("{memory:.0} % utilisée"),
        penalty: 15,
    });
    let failed = run_lossy("systemctl", &["--failed", "--no-legend", "--plain"]);
    let failed = count_failed_units(&failed);
    checks.push(Check {
        label: "Services en échec".to_string(),
        ok: Some(failed == 0),
        detail: format!("{failed} unité(s)"),
        penalty: (failed as u32 * 10).min(30),
    });
    let ufw = run_lossy("ufw", &["status"]);
    let mut firewall = parse_ufw_state(&ufw);
    let mut name = "UFW";
    if firewall.is_none() && crate::exec::exists("firewall-cmd") {
        // Fedora, RHEL, openSUSE : firewalld (« running » / « not running »).
        name = "firewalld";
        let state = run_lossy("firewall-cmd", &["--state"]);
        firewall = match state.trim() {
            "running" => Some(true),
            "not running" => Some(false),
            _ => None,
        };
    }
    checks.push(Check {
        label: "Pare-feu".to_string(),
        ok: firewall,
        detail: match firewall {
            Some(true) => format!("{name} actif"),
            Some(false) => format!("{name} inactif"),
            None => "état illisible (droits administrateur requis ou aucun pare-feu géré)".to_string(),
        },
        penalty: 15,
    });
    let reboot = std::path::Path::new("/var/run/reboot-required").exists();
    checks.push(Check {
        label: "Redémarrage requis".to_string(),
        ok: Some(!reboot),
        detail: if reboot { "oui" } else { "non" }.to_string(),
        penalty: 10,
    });
    checks
}

pub fn health() -> Panel {
    let mut panel = Panel::default();
    let checks = health_checks();
    let score = health_score(&checks);
    panel.metric(
        "Score global",
        format!("{score} / 100"),
        if score >= 85 {
            Level::Ok
        } else if score >= 60 {
            Level::Warn
        } else {
            Level::Bad
        },
    );
    let penalties: Vec<f32> = checks.iter().map(|c| if c.ok == Some(false) { c.penalty as f32 } else { 0.0 }).collect();
    let labels: Vec<String> = checks.iter().map(|c| c.label.clone()).collect();
    let rows = checks
        .iter()
        .map(|check| {
            Row::owned(vec![
                check.label.clone(),
                match check.ok {
                    Some(true) => "OK".to_string(),
                    Some(false) => format!("à corriger (−{})", check.penalty),
                    None => "inconnu".to_string(),
                },
                check.detail.clone(),
            ])
            .bad(check.ok == Some(false))
        })
        .collect();
    panel.section(
        Section::new("Contrôles")
            .chart(Chart::bars("Points perdus par contrôle (sur 100)", "", labels, penalties).max_y(30.0))
            .columns(&["Contrôle", "État", "Détail"])
            .rows(rows)
            .action(Cmd::local("Exporter le rapport", export_health, Vec::new())),
    );
    panel
}

fn export_health(_: &[String]) -> Result<String, String> {
    let checks = health_checks();
    let mut text = format!("Santé du système — {}\nScore : {} / 100\n\n", util::now_human(), health_score(&checks));
    for check in &checks {
        text.push_str(&format!(
            "- {} : {} ({})\n",
            check.label,
            match check.ok {
                Some(true) => "OK",
                Some(false) => "à corriger",
                None => "inconnu",
            },
            check.detail
        ));
    }
    let path = home().join("orbisys-sante.txt");
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    Ok(format!("Rapport écrit : {}", path.display()))
}

// ---------------------------------------------------------------------------
// Benchmark
// ---------------------------------------------------------------------------

/// Opérations par seconde sur une boucle arithmétique de ~300 ms.
pub fn bench_cpu() -> f64 {
    let start = Instant::now();
    let mut operations: u64 = 0;
    let mut accumulator: u64 = 1;
    while start.elapsed() < Duration::from_millis(300) {
        for index in 0..100_000u64 {
            accumulator = accumulator.wrapping_mul(6364136223846793005).wrapping_add(index);
        }
        operations += 100_000;
    }
    std::hint::black_box(accumulator);
    operations as f64 / start.elapsed().as_secs_f64()
}

/// Débit d'écriture mémoire en Mio/s.
pub fn bench_memory() -> f64 {
    const SIZE: usize = 128 * 1024 * 1024;
    let start = Instant::now();
    let mut buffer = vec![0u8; SIZE];
    for (index, byte) in buffer.iter_mut().enumerate() {
        *byte = (index & 0xff) as u8;
    }
    std::hint::black_box(&buffer);
    (SIZE as f64 / 1_048_576.0) / start.elapsed().as_secs_f64().max(1e-6)
}

/// Débit d'écriture disque en Mio/s (fichier temporaire de 64 Mio, supprimé).
pub fn bench_disk() -> Result<f64, String> {
    let path = std::env::temp_dir().join(format!("orbisys-bench-{}", std::process::id()));
    let chunk = vec![0xA5u8; 1024 * 1024];
    let start = Instant::now();
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&path)?;
        for _ in 0..64 {
            file.write_all(&chunk)?;
        }
        file.sync_all()
    })();
    let elapsed = start.elapsed().as_secs_f64().max(1e-6);
    let _ = std::fs::remove_file(&path);
    result.map_err(|error| error.to_string())?;
    Ok(64.0 / elapsed)
}

pub fn benchmark() -> Panel {
    let mut panel = Panel::default();
    let cpu = bench_cpu();
    panel.metric("Processeur", format!("{:.1} M op/s", cpu / 1e6), Level::Info);
    let memory = bench_memory();
    panel.metric("Mémoire (écriture)", format!("{memory:.0} Mio/s"), Level::Info);
    let disk_speed = match bench_disk() {
        Ok(speed) => {
            panel.metric("Disque (écriture)", format!("{speed:.0} Mio/s"), Level::Info);
            Some(speed)
        }
        Err(error) => {
            panel.error(format!("disque : {error}"));
            None
        }
    };
    let mut labels = vec!["Mémoire (écriture)".to_string()];
    let mut values = vec![memory as f32];
    if let Some(speed) = disk_speed {
        labels.push("Disque (écriture)".to_string());
        values.push(speed as f32);
    }
    panel.section(Section::new("Résultats").chart(Chart::bars("Débits mesurés (Mio/s)", "", labels, values)));
    panel.section(
        Section::new("Méthode").text(
            "CPU : boucle arithmétique de 300 ms, en opérations par seconde.\n\
             Mémoire : remplissage de 128 Mio.\n\
             Disque : écriture synchronisée de 64 Mio dans le dossier temporaire, supprimée ensuite.\n\
             Relancer l'écran refait la mesure."
                .to_string(),
        ),
    );
    panel
}

// ---------------------------------------------------------------------------
// Envoi des rapports
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Delivery {
    pub kind: String,
    pub target: String,
}

const DELIVERY_FILE: &str = "delivery.json";

/// Valide un couple (type, cible) de livraison.
pub fn validate_delivery(kind: &str, target: &str) -> Result<(), String> {
    match kind {
        "webhook" if valid_http_url(target) => Ok(()),
        "webhook" => Err("URL http(s) invalide".to_string()),
        "email" if valid_email(target) => Ok(()),
        "email" => Err("adresse e-mail invalide".to_string()),
        _ => Err("type inconnu : webhook ou email".to_string()),
    }
}

fn save_delivery(values: &[String]) -> Result<String, String> {
    let [kind, target] = values else {
        return Err("type et cible attendus".to_string());
    };
    validate_delivery(kind, target)?;
    save_json(
        DELIVERY_FILE,
        &Delivery {
            kind: kind.clone(),
            target: target.clone(),
        },
    )?;
    Ok("Livraison enregistrée".to_string())
}

fn build_delivery(values: &[String]) -> Result<Cmd, String> {
    let [kind, target] = values else {
        return Err("type et cible attendus".to_string());
    };
    validate_delivery(kind.trim(), target.trim())?;
    Ok(Cmd::local(
        "Livraison",
        save_delivery,
        vec![kind.trim().to_string(), target.trim().to_string()],
    ))
}

/// Échappe une valeur pour un fichier de configuration curl (`\\` et `"`).
pub fn curl_config_quote(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Texte du rapport d'état envoyé.
pub fn status_report() -> String {
    let checks = health_checks();
    let memory = disk::memory();
    format!(
        "OrbiSys — {}\nSanté : {}/100\nMémoire : {:.0} %\nDisque : {:.0} %\nCharge : {:?}",
        util::now_human(),
        health_score(&checks),
        memory.percent(),
        disk_percent(),
        disk::load_average()
    )
}

fn send_delivery(_: &[String]) -> Result<String, String> {
    let delivery: Delivery = load_json(DELIVERY_FILE).ok_or("aucune livraison configurée")?;
    validate_delivery(&delivery.kind, &delivery.target)?;
    let body = status_report();
    match delivery.kind.as_str() {
        "webhook" => {
            use std::process::{Command, Stdio};
            let payload = serde_json::json!({ "text": body }).to_string();
            // L'URL est un secret (jeton dans le chemin) : elle passe par la
            // configuration lue sur l'entrée standard de curl, jamais en argument
            // (les arguments sont lisibles par tous via /proc/<pid>/cmdline).
            let config = format!(
                "url = \"{}\"\nrequest = \"POST\"\nheader = \"Content-Type: application/json\"\ndata = \"{}\"\n",
                curl_config_quote(&delivery.target),
                curl_config_quote(&payload)
            );
            let mut child = Command::new("curl")
                .args(["-sS", "--max-time", "20", "--fail", "-K", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| format!("curl indisponible : {error}"))?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(config.as_bytes()).map_err(|error| error.to_string())?;
            }
            let output = child.wait_with_output().map_err(|error| error.to_string())?;
            if output.status.success() {
                Ok("Webhook envoyé".to_string())
            } else {
                // Le message d'erreur de curl peut contenir l'URL : on ne le répète pas.
                Err(format!("échec de l'envoi (code {})", output.status.code().unwrap_or(-1)))
            }
        }
        _ => {
            use std::process::{Command, Stdio};
            let mut child = Command::new("mail")
                .args(["-s", "OrbiSys — rapport", &delivery.target])
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|error| format!("mail indisponible : {error}"))?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(body.as_bytes()).map_err(|error| error.to_string())?;
            }
            let status = child.wait().map_err(|error| error.to_string())?;
            if status.success() {
                Ok("E-mail envoyé".to_string())
            } else {
                Err("la commande mail a échoué".to_string())
            }
        }
    }
}

pub fn delivery() -> Panel {
    let mut panel = Panel::default();
    let current: Option<Delivery> = load_json(DELIVERY_FILE);
    let (kind, target) = current
        .as_ref()
        .map(|d| (d.kind.clone(), d.target.clone()))
        .unwrap_or_else(|| ("webhook".to_string(), String::new()));
    panel.metric(
        "Livraison",
        match &current {
            Some(d) => format!("{} → {}", d.kind, util::ellipsis(&d.target, 40)),
            None => "non configurée".to_string(),
        },
        if current.is_some() { Level::Ok } else { Level::Warn },
    );
    panel.metric(
        "curl",
        if crate::exec::exists("curl") { "présent" } else { "absent" },
        Level::Info,
    );
    panel.metric(
        "mail",
        if crate::exec::exists("mail") { "présent" } else { "absent" },
        Level::Info,
    );
    let mut section = Section::new("Destination")
        .note("Type « webhook » (HTTP POST, JSON) ou « email » (commande mail). Le rapport contient santé, mémoire, disque et charge.")
        .form(Form {
            fields: vec![
                Field::new("Type", "webhook ou email").with_default(&kind),
                Field::new("Cible", "URL ou adresse").with_default(&target),
            ],
            submit: "Enregistrer".to_string(),
            build: build_delivery,
        });
    if current.is_some() {
        section = section.action(
            Cmd::local("Envoyer un test", send_delivery, Vec::new())
                .confirm("Envoyer le rapport d'état à la destination configurée ?"),
        );
    }
    panel.section(section);
    panel
}

// ---------------------------------------------------------------------------
// Rapports planifiés (minuterie systemd utilisateur)
// ---------------------------------------------------------------------------

pub const REPORT_UNIT: &str = "orbisys-report";

pub fn frequency_ok(frequency: &str) -> bool {
    matches!(frequency, "daily" | "weekly" | "monthly")
}

/// Argument sûr à placer entre guillemets dans `ExecStart=`.
pub fn unit_arg_ok(text: &str) -> bool {
    safe_abs_path(text) && !text.chars().any(|ch| "\"\\%$`';".contains(ch))
}

/// Texte des unités `(service, minuterie)` pour un exécutable, un dossier et une fréquence.
pub fn schedule_units(exe: &str, folder: &str, frequency: &str) -> Result<(String, String), String> {
    if !frequency_ok(frequency) {
        return Err("fréquence inconnue : daily, weekly ou monthly".to_string());
    }
    if !unit_arg_ok(exe) {
        return Err("chemin de l'exécutable inutilisable dans une unité systemd".to_string());
    }
    if !unit_arg_ok(folder) {
        return Err("dossier invalide (chemin absolu, sans \" \\ % $ ` ' ;)".to_string());
    }
    let service = format!(
        "[Unit]\nDescription=OrbiSys : rapport d'état planifié\n\n[Service]\nType=oneshot\nExecStart=\"{exe}\" report \"{folder}\"\n"
    );
    let timer = format!(
        "[Unit]\nDescription=Planification du rapport OrbiSys\n\n[Timer]\nOnCalendar={frequency}\nPersistent=true\nRandomizedDelaySec=300\n\n[Install]\nWantedBy=timers.target\n"
    );
    Ok((service, timer))
}

fn user_unit_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| home().join(".config"))
        .join("systemd/user")
}

/// Écrit les deux unités dans `dir` (création exclusive des dossiers, remplacement des fichiers).
pub fn write_units(dir: &std::path::Path, service: &str, timer: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    std::fs::write(dir.join(format!("{REPORT_UNIT}.service")), service).map_err(|error| error.to_string())?;
    std::fs::write(dir.join(format!("{REPORT_UNIT}.timer")), timer).map_err(|error| error.to_string())
}

fn systemctl_user(args: &[&str]) -> Result<String, String> {
    let mut all = vec!["--user"];
    all.extend_from_slice(args);
    let captured = crate::exec::run_with_timeout("systemctl", &all, Duration::from_secs(30)).map_err(|e| e.0)?;
    if captured.ok {
        Ok(captured.text())
    } else {
        Err(captured.error_text())
    }
}

fn install_schedule(args: &[String]) -> Result<String, String> {
    let [frequency, folder] = args else {
        return Err("fréquence et dossier attendus".to_string());
    };
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let (service, timer) = schedule_units(&exe.to_string_lossy(), folder, frequency)?;
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    write_units(&user_unit_dir(), &service, &timer)?;
    systemctl_user(&["daemon-reload"])?;
    systemctl_user(&["enable", "--now", &format!("{REPORT_UNIT}.timer")])?;
    Ok(format!("Rapport {frequency} planifié vers {folder}"))
}

fn remove_schedule(_: &[String]) -> Result<String, String> {
    let _ = systemctl_user(&["disable", "--now", &format!("{REPORT_UNIT}.timer")]);
    for extension in ["timer", "service"] {
        let path = user_unit_dir().join(format!("{REPORT_UNIT}.{extension}"));
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let _ = systemctl_user(&["daemon-reload"]);
    Ok("Planification supprimée".to_string())
}

fn build_schedule(values: &[String]) -> Result<Cmd, String> {
    let [frequency, folder] = values else {
        return Err("fréquence et dossier attendus".to_string());
    };
    let (frequency, folder) = (frequency.trim(), folder.trim());
    // Validation à blanc : mêmes règles qu'à l'installation.
    schedule_units("/usr/bin/orbisys", folder, frequency)?;
    Ok(Cmd::local("Planification", install_schedule, vec![frequency.to_string(), folder.to_string()])
        .confirm(&format!("Générer un rapport {frequency} dans {folder} ?")))
}

pub fn report_schedule() -> Panel {
    let mut panel = Panel::default();
    let timer = format!("{REPORT_UNIT}.timer");
    let enabled = systemctl_user(&["is-enabled", &timer]).unwrap_or_default();
    let active = enabled == "enabled";
    panel.metric("Planification", if active { "active" } else { "inactive" }, if active { Level::Ok } else { Level::Info });
    if active {
        let next = systemctl_user(&["show", "-p", "NextElapseUSecRealtime", "--value", &timer]).unwrap_or_default();
        if !next.is_empty() {
            panel.metric("Prochaine exécution", next, Level::Info);
        }
    }
    let default_folder = home().join("Rapports-OrbiSys").to_string_lossy().to_string();
    let mut section = Section::new("Rapport périodique")
        .note("Crée une minuterie systemd utilisateur qui lance « orbisys report <dossier> » (TXT, CSV, JSON et PDF).")
        .form(Form {
            fields: vec![
                Field::new("Fréquence", "daily, weekly ou monthly").with_default("weekly"),
                Field::new("Dossier", "/chemin/dossier").with_default(&default_folder),
            ],
            submit: "Planifier".to_string(),
            build: build_schedule,
        });
    if active {
        section = section
            .action(Cmd::new("Lancer maintenant", "systemctl", &["--user", "start", &format!("{REPORT_UNIT}.service")]))
            .action(Cmd::local("Supprimer la planification", remove_schedule, Vec::new()).confirm("Désactiver et supprimer la minuterie de rapport ?"));
    }
    panel.section(section);
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_cpu_percent() {
        let before = parse_proc_stat("cpu  100 0 100 800 0 0 0 0 0 0\n").unwrap();
        let after = parse_proc_stat("cpu  200 0 200 1000 0 0 0 0 0 0\n").unwrap();
        // 400 ticks au total, dont 200 inactifs : 50 %.
        assert!((cpu_percent(before, after) - 50.0).abs() < 0.01);
        assert_eq!(cpu_percent(before, before), 0.0);
        assert!(parse_proc_stat("rien").is_none());
    }

    #[test]
    fn parses_net_dev() {
        let text = "Inter-|   Receive\n face |bytes\n  lo: 100 1 0 0 0 0 0 0 100 1 0 0 0 0 0 0\neth0: 5000 10 0 0 0 0 0 0 2500 8 0 0 0 0 0 0\n";
        let parsed = parse_net_dev(text);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1], ("eth0".to_string(), 5000, 2500));
    }

    #[test]
    fn keeps_whole_disks_only() {
        let text = "   8       0 sda 10 0 100 0 20 0 200 0 0 0 0\n   8       1 sda1 10 0 100 0 20 0 200 0 0 0 0\n 259       0 nvme0n1 5 0 50 0 6 0 60 0 0 0 0\n 259       1 nvme0n1p1 5 0 50 0 6 0 60 0 0 0 0\n   7       0 loop0 1 0 1 0 0 0 0 0 0 0 0\n";
        let parsed = parse_diskstats(text);
        let names: Vec<&str> = parsed.iter().map(|(name, _, _)| name.as_str()).collect();
        assert_eq!(names, vec!["sda", "nvme0n1"]);
        assert_eq!(parsed[0].1, 100);
        assert_eq!(parsed[0].2, 200);
    }

    #[test]
    fn thresholds_are_validated() {
        let ok = vec!["80".to_string(), "85".to_string(), "90".to_string(), "75".to_string()];
        assert_eq!(parse_thresholds(&ok).unwrap().mem, 85.0);
        let bad = vec!["0".to_string(), "85".to_string(), "90".to_string(), "75".to_string()];
        assert!(parse_thresholds(&bad).is_err());
        let text = vec!["a".to_string(), "1".to_string(), "1".to_string(), "1".to_string()];
        assert!(parse_thresholds(&text).is_err());
        assert!(parse_thresholds(&ok[..3]).is_err());
        assert!(build_thresholds(&ok).is_ok());
    }

    #[test]
    fn detects_breaches() {
        let limits = Thresholds::default();
        let calm = Measure { cpu: 10.0, mem: 20.0, disk: 30.0, temp: Some(40.0) };
        assert!(breaches(&calm, &limits).is_empty());
        let hot = Measure { cpu: 95.0, mem: 20.0, disk: 91.0, temp: Some(85.0) };
        assert_eq!(breaches(&hot, &limits).len(), 3);
    }

    #[test]
    fn sparkline_maps_range() {
        assert_eq!(sparkline(&[0.0, 100.0]), "▁█");
        assert_eq!(sparkline(&[]), "");
        assert_eq!(sparkline(&[250.0]), "█");
    }

    #[test]
    fn health_score_subtracts_penalties() {
        let checks = vec![
            Check { label: "a".into(), ok: Some(true), detail: String::new(), penalty: 20 },
            Check { label: "b".into(), ok: Some(false), detail: String::new(), penalty: 15 },
            Check { label: "c".into(), ok: None, detail: String::new(), penalty: 50 },
        ];
        assert_eq!(health_score(&checks), 85);
    }

    #[test]
    fn reads_ufw_state() {
        assert_eq!(parse_ufw_state("Status: active\n"), Some(true));
        assert_eq!(parse_ufw_state("Status: inactive\n"), Some(false));
        assert_eq!(parse_ufw_state("État : actif\n"), Some(true));
        assert_eq!(parse_ufw_state("Statut : inactif\n"), Some(false));
        assert_eq!(parse_ufw_state("ERROR: Vous devez être root"), None);
        assert_eq!(parse_ufw_state(""), None);
    }

    #[test]
    fn counts_failed_units() {
        let text = "● foo.service loaded failed failed Foo\n● bar.timer loaded failed failed Bar\n\n2 loaded units listed.\n";
        assert_eq!(count_failed_units(text), 2);
        assert_eq!(count_failed_units(""), 0);
    }

    #[test]
    fn delivery_is_validated() {
        assert!(validate_delivery("webhook", "https://exemple.fr/hook").is_ok());
        assert!(validate_delivery("webhook", "javascript:alert(1)").is_err());
        assert!(validate_delivery("email", "moi@exemple.fr").is_ok());
        assert!(validate_delivery("email", "pas un mail").is_err());
        assert!(validate_delivery("sms", "0600").is_err());
        assert!(build_delivery(&["email".to_string(), "x".to_string()]).is_err());
    }

    #[test]
    fn curl_config_values_are_escaped() {
        assert_eq!(curl_config_quote("https://h/x"), "https://h/x");
        assert_eq!(curl_config_quote("a\"b"), "a\\\"b");
        assert_eq!(curl_config_quote("a\\b"), "a\\\\b");
        assert_eq!(curl_config_quote("a\nb"), "a\\nb");
        assert!(!curl_config_quote("x\"\nurl = \"evil").contains('\n'));
    }

    #[test]
    fn live_ring_is_bounded() {
        let mut ring = std::collections::VecDeque::new();
        for index in 0..300 {
            push_bounded(&mut ring, LiveSample { cpu: index as f32, mem: 0.0, rx: 0.0, tx: 0.0 }, LIVE_CAPACITY);
        }
        assert_eq!(ring.len(), LIVE_CAPACITY);
        assert_eq!(ring.front().unwrap().cpu, 180.0);
        assert_eq!(ring.back().unwrap().cpu, 299.0);
    }

    #[test]
    fn live_charts_have_expected_series() {
        let history = vec![
            LiveSample { cpu: 10.0, mem: 20.0, rx: 1000.0, tx: 500.0 },
            LiveSample { cpu: 30.0, mem: 25.0, rx: 2000.0, tx: 100.0 },
        ];
        let charts = live_charts(&history);
        assert_eq!(charts.len(), 2);
        assert_eq!(charts[0].series[0].values, vec![10.0, 30.0]);
        assert_eq!(charts[0].axis_max(), 100.0);
        assert_eq!(charts[1].unit, "B/s");
        assert_eq!(charts[1].series[0].values, vec![1000.0, 2000.0]);
        assert_eq!(charts[1].axis_max(), 2000.0);
        assert!(live_charts(&[]).iter().all(|chart| chart.series.iter().all(|s| s.values.is_empty())));
    }

    #[test]
    fn monitor_panel_contains_charts() {
        let panel = monitor();
        assert!(panel.sections.iter().any(|section| section.charts.len() == 2));
    }

    #[test]
    fn csv_has_header() {
        let csv = samples_to_csv(&[Sample { timestamp: "t".into(), cpu: 1.0, mem: 2.0, disk: 3.0 }]);
        assert!(csv.starts_with("horodatage;cpu;memoire;disque\n"));
        assert!(csv.contains("t;1.0;2.0;3.0"));
    }

    #[test]
    fn cpu_benchmark_reports_positive_rate() {
        assert!(bench_cpu() > 0.0);
    }

    #[test]
    fn builds_schedule_units() {
        let (service, timer) = schedule_units("/usr/bin/orbisys", "/home/u/Rapports", "weekly").unwrap();
        assert!(service.contains("ExecStart=\"/usr/bin/orbisys\" report \"/home/u/Rapports\""));
        assert!(service.contains("Type=oneshot"));
        assert!(timer.contains("OnCalendar=weekly"));
        assert!(timer.contains("WantedBy=timers.target"));
    }

    #[test]
    fn schedule_rejects_injection() {
        let ok = "/usr/bin/orbisys";
        assert!(schedule_units(ok, "/home/u/x\"; rm -rf /", "daily").is_err());
        assert!(schedule_units(ok, "/home/u/%h", "daily").is_err());
        assert!(schedule_units(ok, "/home/u/a\nExecStartPost=/bin/sh", "daily").is_err());
        assert!(schedule_units(ok, "relatif", "daily").is_err());
        assert!(schedule_units(ok, "/home/u/../etc", "daily").is_err());
        assert!(schedule_units(ok, "/home/u/x", "*-*-* *:*:*").is_err());
        assert!(schedule_units("/tmp/a\"b", "/home/u/x", "daily").is_err());
        assert!(build_schedule(&["weekly".into(), "/home/u/ok".into()]).unwrap().confirm.is_some());
        assert!(build_schedule(&["hourly".into(), "/home/u/ok".into()]).is_err());
    }

    #[test]
    fn writes_unit_files() {
        let dir = std::env::temp_dir().join(format!("um-sched-{}", std::process::id())).join("systemd/user");
        write_units(&dir, "S", "T").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("orbisys-report.service")).unwrap(), "S");
        assert_eq!(std::fs::read_to_string(dir.join("orbisys-report.timer")).unwrap(), "T");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap());
    }
}
