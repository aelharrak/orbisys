//! Optimisation : TRIM des disques SSD, gouvernance CPU, mémoire et
//! services les plus lents au démarrage.

use crate::exec;
use serde::Serialize;

/// Un disque et son support du TRIM.
#[derive(Debug, Clone, Serialize)]
pub struct DiskTrim {
    pub device: String,
    pub model: String,
    pub rotational: bool,
    pub discard: bool,
}

/// Gouvernance du processeur.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CpuInfo {
    pub governor: String,
    pub available: Vec<String>,
    pub driver: String,
}

/// État de la mémoire.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryInfo {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub dirty: u64,
    pub swappiness: String,
    pub cache_pressure: String,
}

/// Un service mesuré au démarrage.
#[derive(Debug, Clone, Serialize)]
pub struct BootService {
    pub unit: String,
    pub seconds: f64,
}

/// État de la page Optimisation.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OptimizeReport {
    pub disks: Vec<DiskTrim>,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub trim_timer: String,
    pub boot: Vec<BootService>,
    pub cpupower: bool,
}

/// Analyse `/proc/meminfo` (valeurs en kio converties en octets).
pub fn parse_meminfo(text: &str) -> std::collections::BTreeMap<String, u64> {
    let mut values = std::collections::BTreeMap::new();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let number = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        values.insert(key.trim().to_string(), number * 1024);
    }
    values
}

/// Analyse la ligne `[...] mq-deadline kyber none` d'un planificateur d'E/S.
pub fn parse_scheduler(line: &str) -> (String, Vec<String>) {
    let mut current = String::new();
    let mut available = Vec::new();
    for token in line.split_whitespace() {
        if let Some(name) = token.strip_prefix('[').and_then(|value| value.strip_suffix(']')) {
            current = name.to_string();
            available.push(name.to_string());
        } else {
            available.push(token.to_string());
        }
    }
    (current, available)
}

/// Analyse `systemctl --no-pager --no-legend list-units` ? Non : `systemd-analyze blame`.
pub fn parse_blame(text: &str) -> Vec<BootService> {
    let mut services = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        // `450ms` doit être traité avant `1.234s`, sinon le suffixe `s`
        // laisse `450m` et la valeur est perdue.
        let (number, millis) = if let Some(value) = fields[0].strip_suffix("ms") {
            (value, true)
        } else if let Some(value) = fields[0].strip_suffix('s') {
            (value, false)
        } else {
            continue;
        };
        let Ok(value) = number.parse::<f64>() else {
            continue;
        };
        let seconds = if millis { value / 1000.0 } else { value };
        services.push(BootService {
            unit: fields[1].to_string(),
            seconds,
        });
    }
    services.sort_by(|a, b| b.seconds.partial_cmp(&a.seconds).unwrap_or(std::cmp::Ordering::Equal));
    services.truncate(12);
    services
}

fn read_trim(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn disks() -> Vec<DiskTrim> {
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return Vec::new();
    };
    let mut disks = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("loop") || name.starts_with("zram") || name.starts_with("dm-") {
            continue;
        }
        let base = format!("/sys/block/{name}");
        let rotational = read_trim(&format!("{base}/queue/rotational")) == "1";
        let discard = read_trim(&format!("{base}/queue/discard_max_bytes"))
            .parse::<u64>()
            .map(|value| value > 0)
            .unwrap_or(false);
        let model = read_trim(&format!("{base}/device/model"));
        disks.push(DiskTrim {
            device: format!("/dev/{name}"),
            model: if model.is_empty() { name } else { model },
            rotational,
            discard,
        });
    }
    disks.sort_by(|a, b| a.device.cmp(&b.device));
    disks
}

fn governor() -> CpuInfo {
    let base = "/sys/devices/system/cpu/cpu0/cpufreq";
    let current = read_trim(&format!("{base}/scaling_governor"));
    let available = read_trim(&format!("{base}/scaling_available_governors"));
    CpuInfo {
        governor: current,
        available: available.split_whitespace().map(str::to_string).collect(),
        driver: read_trim(&format!("{base}/scaling_driver")),
    }
}

/// Lit l'état d'optimisation de la machine.
pub fn report() -> OptimizeReport {
    let meminfo = std::fs::read_to_string("/proc/meminfo")
        .map(|text| parse_meminfo(&text))
        .unwrap_or_default();
    let get = |key: &str| meminfo.get(key).copied().unwrap_or(0);
    let swap_total = get("SwapTotal");
    let swap_free = get("SwapFree");

    let trim_timer = exec::run("systemctl", &["is-enabled", "fstrim.timer"])
        .map(|captured| captured.text())
        .unwrap_or_default();

    let boot = exec::run("systemd-analyze", &["blame"])
        .map(|captured| parse_blame(&captured.stdout))
        .unwrap_or_default();

    OptimizeReport {
        disks: disks(),
        cpu: governor(),
        memory: MemoryInfo {
            total: get("MemTotal"),
            available: get("MemAvailable"),
            swap_total,
            swap_used: swap_total.saturating_sub(swap_free),
            dirty: get("Dirty"),
            swappiness: read_trim("/proc/sys/vm/swappiness"),
            cache_pressure: read_trim("/proc/sys/vm/vfs_cache_pressure"),
        },
        trim_timer,
        boot,
        cpupower: exec::exists("cpupower"),
    }
}

/// Lance un TRIM sur tous les systèmes de fichiers montés (Polkit).
pub fn run_trim() -> Result<String, String> {
    if !exec::exists("fstrim") {
        return Err("fstrim n'est pas disponible (paquet util-linux)".to_string());
    }
    let output = exec::run_privileged("fstrim", &["-av".to_string()])?;
    Ok(if output.is_empty() {
        "TRIM terminé".to_string()
    } else {
        output
    })
}

/// Change la gouvernance CPU (Polkit).
pub fn set_governor(governor: &str) -> Result<String, String> {
    let allowed = ["powersave", "performance", "ondemand", "conservative", "schedutil"];
    if !allowed.contains(&governor) {
        return Err(format!("gouvernance refusée : {governor}"));
    }
    if !exec::exists("cpupower") {
        return Err(
            "cpupower est requis pour changer la gouvernance (paquet linux-tools)".to_string(),
        );
    }
    exec::run_privileged(
        "cpupower",
        &["frequency-set".to_string(), "-g".to_string(), governor.to_string()],
    )?;
    Ok(format!("gouvernance CPU : {governor}"))
}

/// Vrai si le disque est un support à état solide.
pub fn is_solid_state(disk: &DiskTrim) -> bool {
    !disk.rotational
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo_in_bytes() {
        let text = "MemTotal:       16384000 kB\nMemAvailable:    8192000 kB\nSwapTotal:       2097152 kB\nSwapFree:        1048576 kB\nDirty:               128 kB\n";
        let values = parse_meminfo(text);
        assert_eq!(values["MemTotal"], 16_384_000 * 1024);
        assert_eq!(values["MemAvailable"], 8_192_000 * 1024);
        assert_eq!(values["Dirty"], 128 * 1024);
    }

    #[test]
    fn parses_io_scheduler() {
        let (current, available) = parse_scheduler("[mq-deadline] kyber bfq none");
        assert_eq!(current, "mq-deadline");
        assert_eq!(available.len(), 4);
        assert!(available.contains(&"bfq".to_string()));
    }

    #[test]
    fn parses_boot_blame_sorted() {
        let text = "  1.234s NetworkManager-wait-online.service\n 12.500s snapd.service\n   450ms systemd-udev-settle.service\n";
        let services = parse_blame(text);
        assert_eq!(services[0].unit, "snapd.service");
        assert!((services[0].seconds - 12.5).abs() < 0.001);
        assert!((services[2].seconds - 0.45).abs() < 0.001);
    }

    #[test]
    fn rejects_unknown_governors() {
        assert!(set_governor("turbo; reboot").is_err());
        assert!(set_governor("").is_err());
    }

    #[test]
    fn detects_solid_state_disks() {
        let nvme = DiskTrim {
            device: "/dev/nvme0n1".to_string(),
            model: "SSD".to_string(),
            rotational: false,
            discard: true,
        };
        let hdd = DiskTrim {
            device: "/dev/sda".to_string(),
            model: "HDD".to_string(),
            rotational: true,
            discard: false,
        };
        assert!(is_solid_state(&nvme));
        assert!(!is_solid_state(&hdd));
    }
}
