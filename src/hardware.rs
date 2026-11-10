//! Matériel : processeur, mémoire, carte mère, cartes graphiques,
//! périphériques, batterie et capteurs — en lecture seule.

use crate::exec;
use serde::Serialize;

/// État de la batterie.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Battery {
    pub percent: f32,
    pub status: String,
    pub health: Option<f32>,
}

/// Instantané matériel.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HardwareReport {
    pub cpu_model: String,
    pub logical_cpus: usize,
    pub physical_cores: usize,
    pub architecture: String,
    pub memory_total: u64,
    pub swap_total: u64,
    pub vendor: String,
    pub product: String,
    pub bios: String,
    pub gpus: Vec<String>,
    pub usb_devices: usize,
    pub pci_devices: usize,
    pub battery: Option<Battery>,
    pub temperatures: Vec<(String, f32)>,
}

/// Analyse `/proc/cpuinfo` : (modèle, cœurs physiques, processeurs logiques).
pub fn parse_cpuinfo(text: &str) -> (String, usize, usize) {
    let mut model = String::new();
    let mut logical = 0;
    let mut cores: Vec<(String, String)> = Vec::new();
    let mut physical_id = String::new();
    let mut core_id = String::new();

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "processor" => {
                logical += 1;
                physical_id.clear();
                core_id.clear();
            }
            "model name" if model.is_empty() => model = value.to_string(),
            "physical id" => physical_id = value.to_string(),
            "core id" => core_id = value.to_string(),
            _ => {}
        }
        if !physical_id.is_empty() && !core_id.is_empty() {
            let pair = (physical_id.clone(), core_id.clone());
            if !cores.contains(&pair) {
                cores.push(pair);
            }
        }
    }
    let physical = if cores.is_empty() {
        logical
    } else {
        cores.len()
    };
    (model, physical, logical)
}

/// Lit une valeur `/sys` sous forme de texte.
fn read_sys(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

fn count_entries(path: &str) -> usize {
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}

fn battery() -> Option<Battery> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("BAT") {
            continue;
        }
        let base = format!("/sys/class/power_supply/{name}");
        let percent = read_sys(&format!("{base}/capacity")).parse::<f32>().ok()?;
        let status = read_sys(&format!("{base}/status"));
        let full = read_sys(&format!("{base}/energy_full"))
            .parse::<f32>()
            .ok();
        let design = read_sys(&format!("{base}/energy_full_design"))
            .parse::<f32>()
            .ok();
        let health = match (full, design) {
            (Some(full), Some(design)) if design > 0.0 => Some((full / design * 100.0).min(100.0)),
            _ => None,
        };
        return Some(Battery {
            percent,
            status,
            health,
        });
    }
    None
}

pub fn temperatures() -> Vec<(String, f32)> {
    let mut values = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/thermal") else {
        return values;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("thermal_zone") {
            continue;
        }
        let base = format!("/sys/class/thermal/{name}");
        let Ok(raw) = read_sys(&format!("{base}/temp")).parse::<f32>() else {
            continue;
        };
        let kind = read_sys(&format!("{base}/type"));
        values.push((if kind.is_empty() { name } else { kind }, raw / 1000.0));
    }
    values.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    values.truncate(6);
    values
}

fn gpus() -> Vec<String> {
    if !exec::exists("lspci") {
        return Vec::new();
    }
    let Ok(captured) = exec::run("lspci", &[]) else {
        return Vec::new();
    };
    captured
        .stdout
        .lines()
        .filter(|line| line.contains("VGA compatible") || line.contains("3D controller"))
        .map(|line| {
            line.split_once(": ")
                .map(|(_, rest)| rest.to_string())
                .unwrap_or_else(|| line.to_string())
        })
        .collect()
}

/// Collecte l'état matériel (aucune écriture).
pub fn report() -> HardwareReport {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let (cpu_model, physical_cores, logical_cpus) = parse_cpuinfo(&cpuinfo);
    let memory = crate::disk::memory();
    let architecture = exec::run("uname", &["-m"])
        .map(|captured| captured.text())
        .unwrap_or_default();

    HardwareReport {
        cpu_model,
        logical_cpus,
        physical_cores,
        architecture,
        memory_total: memory.total,
        swap_total: memory.swap_total,
        vendor: read_sys("/sys/class/dmi/id/sys_vendor"),
        product: read_sys("/sys/class/dmi/id/product_name"),
        bios: read_sys("/sys/class/dmi/id/bios_version"),
        gpus: gpus(),
        usb_devices: count_entries("/sys/bus/usb/devices"),
        pci_devices: count_entries("/sys/bus/pci/devices"),
        battery: battery(),
        temperatures: temperatures(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cpuinfo_cores() {
        let text = "processor\t: 0\nmodel name\t: AMD Ryzen 7 5800U\nphysical id\t: 0\ncore id\t\t: 0\n\n\
                    processor\t: 1\nmodel name\t: AMD Ryzen 7 5800U\nphysical id\t: 0\ncore id\t\t: 1\n\n\
                    processor\t: 2\nmodel name\t: AMD Ryzen 7 5800U\nphysical id\t: 0\ncore id\t\t: 0\n";
        let (model, physical, logical) = parse_cpuinfo(text);
        assert_eq!(model, "AMD Ryzen 7 5800U");
        assert_eq!(logical, 3);
        assert_eq!(physical, 2);
    }

    #[test]
    fn report_is_readable_on_linux() {
        let report = report();
        assert!(report.logical_cpus >= 1);
        assert!(report.memory_total > 0);
    }

    #[test]
    fn handles_missing_cpuinfo_fields() {
        let (model, physical, logical) = parse_cpuinfo("processor\t: 0\n");
        assert!(model.is_empty());
        assert_eq!(logical, 1);
        assert_eq!(physical, 1);
    }
}
