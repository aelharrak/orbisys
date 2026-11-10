//! Profils de performance : gouvernance CPU, swap, THP et réglages de latence.

use crate::exec;
use serde::Serialize;

/// Un profil de performance prédéfini.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Profile {
    pub name: &'static str,
    pub governor: &'static str,
    pub swappiness: u32,
    pub thp: &'static str,
    pub description: &'static str,
}

/// Profils proposés.
pub const PROFILES: [Profile; 3] = [
    Profile {
        name: "Réactif",
        governor: "schedutil",
        swappiness: 60,
        thp: "madvise",
        description: "Équilibre bureau : fréquence à la demande, swap modéré.",
    },
    Profile {
        name: "Performance",
        governor: "performance",
        swappiness: 10,
        thp: "always",
        description: "Fréquence maximale, swap minimal, pages géantes systématiques.",
    },
    Profile {
        name: "Économie",
        governor: "powersave",
        swappiness: 100,
        thp: "madvise",
        description: "Batterie et silence : fréquence basse, swap agressif.",
    },
];

/// Un périphérique de swap.
#[derive(Debug, Clone, Serialize)]
pub struct SwapDevice {
    pub path: String,
    pub kind: String,
    pub size_kb: u64,
    pub used_kb: u64,
}

/// État actuel de la performance.
#[derive(Debug, Clone, Default)]
pub struct PerfReport {
    pub governors: Vec<String>,
    pub per_cpu: Vec<(String, String)>,
    pub turbo: Option<String>,
    pub swappiness: Option<String>,
    pub thp: Option<String>,
    pub swaps: Vec<SwapDevice>,
    pub cpupower: bool,
    pub errors: Vec<String>,
}

impl PerfReport {
    /// Gouvernance majoritaire (celle de la majorité des cœurs).
    pub fn dominant_governor(&self) -> String {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (_, governor) in &self.per_cpu {
            match counts.iter_mut().find(|(name, _)| name == governor) {
                Some((_, count)) => *count += 1,
                None => counts.push((governor.clone(), 1)),
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(name, _)| name)
            .unwrap_or_else(|| "—".to_string())
    }

    pub fn swap_bytes(&self) -> u64 {
        self.swaps.iter().map(|device| device.size_kb).sum::<u64>() * 1024
    }
}

/// Analyse `/proc/swaps`.
pub fn parse_swaps(text: &str) -> Vec<SwapDevice> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 {
                return None;
            }
            Some(SwapDevice {
                path: fields[0].to_string(),
                kind: fields[1].to_string(),
                size_kb: fields[2].parse().unwrap_or(0),
                used_kb: fields[3].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// Extrait le mode actif de `/sys/kernel/mm/transparent_hugepage/enabled`.
pub fn parse_thp(text: &str) -> Option<String> {
    let start = text.find('[')?;
    let end = text[start..].find(']')?;
    Some(text[start + 1..start + end].to_string())
}

/// Un profil par son nom exact.
pub fn profile(name: &str) -> Option<Profile> {
    PROFILES
        .iter()
        .copied()
        .find(|profile| profile.name.eq_ignore_ascii_case(name.trim()))
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

/// Lit un réglage sysctl via `/proc/sys`.
fn proc_sys(key: &str) -> Option<String> {
    read(&format!("/proc/sys/{}", key.replace('.', "/")))
}

/// Collecte l'état de performance.
pub fn report() -> PerfReport {
    let mut report = PerfReport {
        cpupower: exec::exists("cpupower"),
        ..PerfReport::default()
    };
    report.governors = read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors")
        .map(|text| text.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    for index in 0..256 {
        let base = format!("/sys/devices/system/cpu/cpu{index}");
        if !std::path::Path::new(&base).exists() {
            continue;
        }
        if let Some(governor) = read(&format!("{base}/cpufreq/scaling_governor")) {
            report.per_cpu.push((format!("cpu{index}"), governor));
        }
    }
    report.turbo = read("/sys/devices/system/cpu/intel_pstate/no_turbo");
    report.swappiness = proc_sys("vm.swappiness");
    report.thp = read("/sys/kernel/mm/transparent_hugepage/enabled").and_then(|text| parse_thp(&text));
    if let Ok(text) = std::fs::read_to_string("/proc/swaps") {
        report.swaps = parse_swaps(&text);
    }
    if report.per_cpu.is_empty() {
        report
            .errors
            .push("cpufreq indisponible : gouvernance non exposée".to_string());
    }
    report
}

/// Applique un gouverneur CPU à tous les cœurs.
pub fn set_governor(governor: &str) -> Result<String, String> {
    let allowed = read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors")
        .unwrap_or_default();
    if !allowed.split_whitespace().any(|item| item == governor) {
        return Err(format!(
            "gouverneur refusé : {governor} (disponibles : {})",
            if allowed.is_empty() { "aucun" } else { &allowed }
        ));
    }
    exec::run_privileged(
        "cpupower",
        &[
            "frequency-set".to_string(),
            "-g".to_string(),
            governor.to_string(),
        ],
    )?;
    Ok(format!("gouverneur appliqué : {governor}"))
}

/// Règle `vm.swappiness` (0–100).
pub fn set_swappiness(value: u32) -> Result<String, String> {
    if value > 100 {
        return Err(format!("swappiness hors bornes : {value}"));
    }
    exec::run_privileged(
        "sysctl",
        &["-w".to_string(), format!("vm.swappiness={value}")],
    )?;
    Ok(format!("vm.swappiness = {value}"))
}

/// Règle le mode des transparent huge pages.
pub fn set_thp(mode: &str) -> Result<String, String> {
    if !matches!(mode, "always" | "madvise" | "never") {
        return Err(format!("mode THP refusé : {mode}"));
    }
    let path = "/sys/kernel/mm/transparent_hugepage/enabled";
    if !std::path::Path::new(path).exists() {
        return Err("transparent_hugepage non exposé par ce noyau".to_string());
    }
    exec::run_privileged(
        "sysctl",
        &[
            "-w".to_string(),
            format!("vm.transparent_hugepage={mode}"),
        ],
    )?;
    Ok(format!("THP = {mode}"))
}

/// Applique un profil complet et rapporte chaque étape.
pub fn apply_profile(name: &str) -> Result<String, String> {
    let profile = profile(name).ok_or_else(|| format!("profil inconnu : {name}"))?;
    let mut steps: Vec<String> = Vec::new();
    match set_governor(profile.governor) {
        Ok(message) => steps.push(message),
        Err(error) => steps.push(format!("gouverneur : échec ({error})")),
    }
    match set_swappiness(profile.swappiness) {
        Ok(message) => steps.push(message),
        Err(error) => steps.push(format!("swappiness : échec ({error})")),
    }
    match set_thp(profile.thp) {
        Ok(message) => steps.push(message),
        Err(error) => steps.push(format!("THP : {error}")),
    }
    Ok(format!("profil « {} » — {}", profile.name, steps.join(" · ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_swap_devices() {
        let text = "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n/swapfile                               file\t\t2097148\t\t1024\t\t-2\n/dev/sda3                               partition\t8388604\t\t0\t\t-3\n";
        let swaps = parse_swaps(text);
        assert_eq!(swaps.len(), 2);
        assert_eq!(swaps[0].path, "/swapfile");
        assert_eq!(swaps[0].size_kb, 2097148);
        assert_eq!(swaps[1].kind, "partition");
    }

    #[test]
    fn parses_active_thp_mode() {
        assert_eq!(
            parse_thp("always [madvise] never\n"),
            Some("madvise".to_string())
        );
        assert_eq!(parse_thp("always madvise [never]\n"), Some("never".to_string()));
        assert_eq!(parse_thp(""),
            None);
    }

    #[test]
    fn finds_profiles_ignoring_case() {
        assert_eq!(profile("performance").unwrap().governor, "performance");
        assert_eq!(profile(" Économie ").unwrap().swappiness, 100);
        assert!(profile("turbo").is_none());
        assert!(apply_profile("inexistant").is_err());
    }

    #[test]
    fn rejects_out_of_range_swappiness() {
        assert!(set_swappiness(101).is_err());
        assert!(set_thp("parfois").is_err());
    }

    #[test]
    fn dominant_governor_counts_cores() {
        let report = PerfReport {
            per_cpu: vec![
                ("cpu0".to_string(), "schedutil".to_string()),
                ("cpu1".to_string(), "schedutil".to_string()),
                ("cpu2".to_string(), "performance".to_string()),
            ],
            ..PerfReport::default()
        };
        assert_eq!(report.dominant_governor(), "schedutil");
        assert_eq!(PerfReport::default().dominant_governor(), "—");
    }
}
