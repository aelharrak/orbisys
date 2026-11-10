//! Stockage et mémoire : montages (`df`), mémoire (`/proc/meminfo`),
//! charge moyenne et disponibilité d'un outil optionnel.

use crate::exec;
use std::path::Path;

/// Un système de fichiers monté.
#[derive(Debug, Clone)]
pub struct Mount {
    pub source: String,
    pub fstype: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub percent: f32,
    pub target: String,
}

impl Mount {
    /// `true` si le montage est presque plein (>= 90 %).
    pub fn critical(&self) -> bool {
        self.percent >= 90.0
    }
}

/// Liste des montages réels, sans systèmes virtuels.
pub fn mounts() -> Vec<Mount> {
    let captured = exec::run(
        "df",
        &[
            "-B1",
            "-x",
            "tmpfs",
            "-x",
            "devtmpfs",
            "-x",
            "squashfs",
            "-x",
            "overlay",
            "--output=source,fstype,size,used,avail,pcent,target",
        ],
    );
    if let Ok(captured) = captured {
        // L'en-tête dépend de la langue : on se fie au nombre de lignes.
        if captured.ok && captured.stdout.lines().nth(1).is_some() {
            return parse_df_output(&captured.stdout);
        }
    }
    // Repli POSIX pour les `df` minimalistes (busybox).
    exec::run("df", &["-B1", "-P"])
        .map(|captured| parse_df_posix(&captured.stdout))
        .unwrap_or_default()
}

/// Analyse la sortie de `df --output=…`.
pub fn parse_df_output(text: &str) -> Vec<Mount> {
    let mut mounts = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 7 {
            continue;
        }
        let percent = fields[5].trim_end_matches('%').parse::<f32>().unwrap_or(0.0);
        mounts.push(Mount {
            source: fields[0].to_string(),
            fstype: fields[1].to_string(),
            total: fields[2].parse().unwrap_or(0),
            used: fields[3].parse().unwrap_or(0),
            available: fields[4].parse().unwrap_or(0),
            percent,
            target: fields[6..].join(" "),
        });
    }
    mounts
}

/// Analyse la sortie POSIX de `df -P` (source taille utilisé dispo % cible).
pub fn parse_df_posix(text: &str) -> Vec<Mount> {
    let mut mounts = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        let percent = fields[4].trim_end_matches('%').parse::<f32>().unwrap_or(0.0);
        mounts.push(Mount {
            source: fields[0].to_string(),
            fstype: String::new(),
            total: fields[1].parse().unwrap_or(0),
            used: fields[2].parse().unwrap_or(0),
            available: fields[3].parse().unwrap_or(0),
            percent,
            target: fields[5..].join(" "),
        });
    }
    mounts
}

/// État de la mémoire physique et d'échange.
#[derive(Debug, Clone, Copy, Default)]
pub struct Memory {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

impl Memory {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.used() as f32 * 100.0 / self.total as f32
        }
    }

    pub fn swap_used(&self) -> u64 {
        self.swap_total.saturating_sub(self.swap_free)
    }
}

/// Lit `/proc/meminfo`.
pub fn memory() -> Memory {
    std::fs::read_to_string("/proc/meminfo")
        .map(|text| parse_meminfo(&text))
        .unwrap_or_default()
}

/// Analyse `/proc/meminfo` (les valeurs sont en kio).
pub fn parse_meminfo(text: &str) -> Memory {
    let mut memory = Memory::default();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(key) = fields.next() else { continue };
        let value = fields
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
            * 1024;
        match key {
            "MemTotal:" => memory.total = value,
            "MemAvailable:" => memory.available = value,
            "SwapTotal:" => memory.swap_total = value,
            "SwapFree:" => memory.swap_free = value,
            _ => {}
        }
    }
    memory
}

/// Charge moyenne sur 1, 5 et 15 minutes.
pub fn load_average() -> (f32, f32, f32) {
    let text = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let mut values = text.split_whitespace();
    let parse = |value: Option<&str>| {
        value
            .and_then(|value| value.replace(',', ".").parse::<f32>().ok())
            .unwrap_or(0.0)
    };
    (parse(values.next()), parse(values.next()), parse(values.next()))
}

/// Temps depuis le démarrage, en secondes.
pub fn uptime_seconds() -> u64 {
    let text = std::fs::read_to_string("/proc/uptime").unwrap_or_default();
    text.split_whitespace()
        .next()
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| value as u64)
        .unwrap_or(0)
}

/// Taille réelle d'un dossier (bornée), 0 s'il est absent.
pub fn directory_usage(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    crate::util::dir_size(path, 200_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_custom_df_output() {
        let text = "Filesystem     Type 1B-blocks       Used   Available Use% Mounted on\n/dev/nvme0n1p2 ext4 500000000000 300000000000 175000000000  64% /\n/dev/sda1      vfat    536870912     102400     536768512   1% /boot/efi\n";
        let mounts = parse_df_output(text);
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].target, "/");
        assert_eq!(mounts[0].fstype, "ext4");
        assert!((mounts[0].percent - 64.0).abs() < f32::EPSILON);
        assert!(!mounts[0].critical());
    }

    #[test]
    fn parses_posix_df_output() {
        let text = "Filesystem 1-blocks Used Available Capacity Mounted on\n/dev/sda2 100 50 50 50% /\n";
        let mounts = parse_df_posix(text);
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].target, "/");
        assert_eq!(mounts[0].available, 50);
    }

    #[test]
    fn parses_meminfo() {
        let text = "MemTotal:       16000000 kB\nMemFree:         2000000 kB\nMemAvailable:    8000000 kB\nSwapTotal:       2000000 kB\nSwapFree:        1500000 kB\n";
        let memory = parse_meminfo(text);
        assert_eq!(memory.total, 16_000_000 * 1024);
        assert_eq!(memory.used(), 8_000_000 * 1024);
        assert_eq!(memory.swap_used(), 500_000 * 1024);
        assert!((memory.percent() - 50.0).abs() < 0.01);
    }

    #[test]
    fn critical_threshold() {
        let mount = Mount {
            source: "/dev/x".into(),
            fstype: "ext4".into(),
            total: 100,
            used: 92,
            available: 8,
            percent: 92.0,
            target: "/data".into(),
        };
        assert!(mount.critical());
    }
}
