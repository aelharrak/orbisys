//! Système et noyaux : paramètres sysctl persistants, swap/zram, noyaux
//! installés et pilotes recommandés.

use crate::exec;
use crate::util;
use serde::Serialize;
use std::path::PathBuf;

const SYSCTL_FILE: &str = "/etc/sysctl.d/99-orbisys.conf";

/// Une valeur sysctl et sa provenance.
#[derive(Debug, Clone, Serialize)]
pub struct Sysctl {
    pub key: String,
    pub value: String,
    pub origin: String,
}

/// Une zone d'échange active.
#[derive(Debug, Clone, Serialize)]
pub struct SwapEntry {
    pub device: String,
    pub kind: String,
    pub size: u64,
    pub used: u64,
    pub priority: String,
}

/// Un noyau installé.
#[derive(Debug, Clone, Serialize)]
pub struct KernelEntry {
    pub version: String,
    pub package: String,
    pub running: bool,
}

/// Un pilote proposé pour un périphérique.
#[derive(Debug, Clone, Serialize)]
pub struct Driver {
    pub device: String,
    pub driver: String,
    pub free: bool,
}

/// État de la page Système & noyaux.
#[derive(Debug, Clone, Default, Serialize)]
pub struct KernelReport {
    pub sysctl: Vec<Sysctl>,
    pub swap: Vec<SwapEntry>,
    pub swappiness: String,
    pub zram: Vec<String>,
    pub kernels: Vec<KernelEntry>,
    pub drivers: Vec<Driver>,
    pub running: String,
}

/// Valeurs proposées par défaut, reprises de la configuration si présente.
pub const PRESETS: [(&str, &str); 3] = [
    ("vm.swappiness", "10"),
    ("vm.max_map_count", "1048576"),
    ("fs.inotify.max_user_watches", "524288"),
];

/// Une clé sysctl valide (`vm.swappiness`, `net.ipv4.ip_forward`).
pub fn valid_sysctl_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && !key.starts_with('-')
        && !key.starts_with('.')
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Analyse un fichier `sysctl.conf` (`clé = valeur`).
pub fn parse_sysctl(text: &str, origin: &str) -> Vec<Sysctl> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !valid_sysctl_key(key) {
            continue;
        }
        entries.push(Sysctl {
            key: key.to_string(),
            value: value.trim().trim_matches('"').to_string(),
            origin: origin.to_string(),
        });
    }
    entries
}

/// Analyse `swapon --show --bytes --noheadings`.
pub fn parse_swapon(text: &str) -> Vec<SwapEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        entries.push(SwapEntry {
            device: fields[0].to_string(),
            kind: fields[1].to_string(),
            size: fields[2].parse().unwrap_or(0),
            used: fields.get(3).and_then(|value| value.parse().ok()).unwrap_or(0),
            priority: fields.get(4).map(|value| value.to_string()).unwrap_or_default(),
        });
    }
    entries
}

/// Analyse la sortie de `dpkg-query` pour les paquets `linux-image-*`.
pub fn parse_kernel_packages(text: &str) -> Vec<String> {
    let mut packages = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        let (package, status) = (fields[0], fields[2]);
        if status != "ii" || !package.starts_with("linux-image-") {
            continue;
        }
        let version = package.trim_start_matches("linux-image-");
        if version == "generic" || !version.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        if !packages.iter().any(|existing| existing == package) {
            packages.push(package.to_string());
        }
    }
    packages.sort();
    packages
}

/// Analyse `ubuntu-drivers devices`.
pub fn parse_drivers(text: &str) -> Vec<Driver> {
    let mut drivers = Vec::new();
    let mut model = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("model:") {
            model = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("driver:") {
            let rest = rest.trim();
            let name = rest.split(" - ").next().unwrap_or(rest).trim().to_string();
            if name.is_empty() {
                continue;
            }
            drivers.push(Driver {
                device: if model.is_empty() {
                    "périphérique".to_string()
                } else {
                    model.clone()
                },
                driver: name,
                free: rest.contains(" free") && !rest.contains("non-free"),
            });
        }
    }
    drivers
}

fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn sysctl_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir("/etc/sysctl.d")
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().map(|ext| ext == "conf").unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn zram_devices() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.starts_with("zram").then_some(name)
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let size: u64 = read_file(&format!("/sys/block/{name}/disksize"))
                .trim()
                .parse()
                .unwrap_or(0);
            let algorithm = read_file(&format!("/sys/block/{name}/comp_algorithm"))
                .split_whitespace()
                .find(|value| value.starts_with('['))
                .map(|value| value.trim_matches(|c| c == '[' || c == ']').to_string())
                .unwrap_or_default();
            if algorithm.is_empty() {
                format!("{name} — {}", util::human_bytes(size))
            } else {
                format!("{name} — {} ({algorithm})", util::human_bytes(size))
            }
        })
        .collect()
}

/// Version du noyau en cours d'exécution.
pub fn running_kernel() -> String {
    exec::run("uname", &["-r"])
        .map(|captured| captured.text())
        .unwrap_or_default()
}

/// Lit l'état complet du système.
pub fn report() -> KernelReport {
    let mut sysctl = parse_sysctl(&read_file("/etc/sysctl.conf"), "/etc/sysctl.conf");
    for file in sysctl_files() {
        let origin = file.display().to_string();
        sysctl.extend(parse_sysctl(&read_file(&origin), &origin));
    }

    let swap_text = exec::run("swapon", &["--show", "--bytes", "--noheadings"])
        .map(|captured| captured.stdout)
        .unwrap_or_default();
    let running = running_kernel();

    let kernel_text = exec::run(
        "dpkg-query",
        &[
            "-W",
            "-f=${Package}\\t${Version}\\t${db:Status-Abbrev}\\n",
            "linux-image-*",
        ],
    )
    .map(|captured| captured.stdout)
    .unwrap_or_default();
    let kernels = parse_kernel_packages(&kernel_text)
        .into_iter()
        .map(|package| {
            let version = package.trim_start_matches("linux-image-").to_string();
            KernelEntry {
                running: !running.is_empty() && running.contains(&version),
                version,
                package,
            }
        })
        .collect();

    let drivers = exec::run("ubuntu-drivers", &["devices"])
        .map(|captured| parse_drivers(&captured.stdout))
        .unwrap_or_default();

    KernelReport {
        sysctl,
        swap: parse_swapon(&swap_text),
        swappiness: read_file("/proc/sys/vm/swappiness").trim().to_string(),
        zram: zram_devices(),
        kernels,
        drivers,
        running,
    }
}

/// Valeurs initiales des trois réglages proposés.
pub fn presets(report: &KernelReport) -> Vec<(String, String)> {
    PRESETS
        .iter()
        .map(|(key, fallback)| {
            let value = report
                .sysctl
                .iter()
                .rev()
                .find(|entry| entry.key == *key)
                .map(|entry| entry.value.clone())
                .unwrap_or_else(|| fallback.to_string());
            (key.to_string(), value)
        })
        .collect()
}

/// Applique et persiste des valeurs sysctl via `/etc/sysctl.d`.
pub fn apply_sysctl(entries: &[(String, String)]) -> Result<String, String> {
    if entries.is_empty() {
        return Err("aucune valeur à appliquer".to_string());
    }
    for (key, value) in entries {
        if !valid_sysctl_key(key) {
            return Err(format!("clé refusée : {key}"));
        }
        if value.contains('\n') || value.contains('\0') {
            return Err(format!("valeur refusée pour {key}"));
        }
    }
    let mut content = String::from("# Généré par OrbiSys\n");
    for (key, value) in entries {
        content.push_str(&format!("{key} = {value}\n"));
    }
    let temporary = util::write_private_temp("um-sysctl", &content)?;
    let installed = exec::run_privileged(
        "install",
        &[
            "-m".to_string(),
            "644".to_string(),
            temporary.display().to_string(),
            SYSCTL_FILE.to_string(),
        ],
    );
    let _ = std::fs::remove_file(&temporary);
    installed?;
    exec::run_privileged(
        "sysctl",
        &["-p".to_string(), SYSCTL_FILE.to_string()],
    )?;
    Ok(format!(
        "{} réglage(s) appliqué(s) et persistés dans {SYSCTL_FILE}",
        entries.len()
    ))
}

/// Noyaux supprimables : tout sauf le noyau en cours et le plus récent.
pub fn removal_candidates(kernels: &[KernelEntry]) -> Vec<String> {
    let mut versions: Vec<&KernelEntry> = kernels.iter().filter(|entry| !entry.running).collect();
    versions.sort_by(|a, b| b.version.cmp(&a.version));
    versions
        .into_iter()
        .skip(1)
        .map(|entry| entry.package.clone())
        .collect()
}

/// Supprime des noyaux (jamais celui en cours d'exécution).
pub fn remove_kernels(packages: &[String]) -> Result<String, String> {
    if packages.is_empty() {
        return Err("aucun noyau à supprimer".to_string());
    }
    let running = running_kernel();
    let mut args = vec!["purge".to_string(), "-y".to_string()];
    for package in packages {
        if !package.starts_with("linux-image-") || !util::valid_package_name(package) {
            return Err(format!("paquet refusé : {package}"));
        }
        let version = package.trim_start_matches("linux-image-");
        if !running.is_empty() && running.contains(version) {
            return Err(format!("refus de supprimer le noyau en cours : {version}"));
        }
        args.push(package.clone());
    }
    exec::run_privileged("apt-get", &args)?;
    Ok(format!("{} noyau(x) supprimé(s)", packages.len()))
}

/// Installe un pilote proposé par `ubuntu-drivers`.
pub fn install_driver(driver: &str) -> Result<String, String> {
    if driver.is_empty()
        || driver.len() > 120
        || !driver
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(format!("pilote refusé : {driver}"));
    }
    if !exec::exists("ubuntu-drivers") {
        return Err("ubuntu-drivers n'est pas disponible".to_string());
    }
    exec::run_privileged(
        "ubuntu-drivers",
        &["install".to_string(), driver.to_string()],
    )?;
    Ok(format!("pilote {driver} installé"))
}

/// Vrai si au moins un périphérique zram est présent.
pub fn zram_present() -> bool {
    !zram_devices().is_empty()
}

/// Active ou désactive zram : module `zram` et service
/// `systemd-zram-setup@zram0.service` (systemd-zram-generator).
pub fn set_zram(enabled: bool) -> Result<String, String> {
    if !exec::exists("systemctl") {
        return Err("systemctl est requis pour piloter zram".to_string());
    }
    if enabled {
        if !exec::exists("modprobe") {
            return Err("modprobe est requis pour charger le module zram".to_string());
        }
        exec::run_privileged("modprobe", &["zram".to_string()])
            .map_err(|error| format!("chargement du module zram : {error}"))?;
        exec::run_privileged(
            "systemctl",
            &[
                "enable".to_string(),
                "--now".to_string(),
                "systemd-zram-setup@zram0.service".to_string(),
            ],
        )
        .map_err(|error| {
            format!(
                "activation de zram : {error} (service systemd-zram-setup@zram0 indisponible : paquet systemd-zram-generator requis)"
            )
        })?;
        Ok("zram activé : /dev/zram0 sert de zone d'échange compressée".to_string())
    } else {
        exec::run_privileged(
            "systemctl",
            &[
                "disable".to_string(),
                "--now".to_string(),
                "systemd-zram-setup@zram0.service".to_string(),
            ],
        )?;
        let _ = exec::run_privileged("swapoff", &["/dev/zram0".to_string()]);
        let _ = exec::run_privileged("modprobe", &["-r".to_string(), "zram".to_string()]);
        Ok("zram désactivé".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysctl_keys_are_validated() {
        assert!(valid_sysctl_key("vm.swappiness"));
        assert!(valid_sysctl_key("net.ipv4.ip_forward"));
        assert!(!valid_sysctl_key(""));
        assert!(!valid_sysctl_key("-x"));
        assert!(!valid_sysctl_key("a;b"));
        assert!(!valid_sysctl_key("a b"));
    }

    #[test]
    fn parses_sysctl_files() {
        let text = "# commentaire\nvm.swappiness=10\n\nnet.ipv4.ip_forward = \"1\"\nligne invalide\n";
        let entries = parse_sysctl(text, "/etc/sysctl.conf");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "vm.swappiness");
        assert_eq!(entries[1].value, "1");
    }

    #[test]
    fn parses_swapon_output() {
        let text = "/swapfile                                file 2147483648 1048576  -2\n/dev/zram0                              partition 4026531840 0 100\n";
        let entries = parse_swapon(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].device, "/swapfile");
        assert_eq!(entries[0].size, 2_147_483_648);
        assert_eq!(entries[1].used, 0);
        assert_eq!(entries[1].priority, "100");
    }

    #[test]
    fn keeps_only_installed_kernel_packages() {
        let text = "linux-image-6.8.0-45-generic\t6.8.0-45.45\tii\nlinux-image-6.8.0-31-generic\t6.8.0-31.31\trc\nlinux-image-generic\t6.8.0.45.45\tii\nlinux-image-6.8.0-45-generic\t6.8.0-45.45\tii\n";
        let packages = parse_kernel_packages(text);
        assert_eq!(packages, vec!["linux-image-6.8.0-45-generic"]);
    }

    #[test]
    fn parses_ubuntu_drivers_devices() {
        let text = "== /sys/devices/pci0000:00/0000:00:02.0 ==\nmodalias: pci:v000010DEd00002504\nmodel: GA106 [GeForce RTX 3060]\ndriver: nvidia-driver-535 - third-party non-free\n";
        let drivers = parse_drivers(text);
        assert_eq!(drivers.len(), 1);
        assert_eq!(drivers[0].driver, "nvidia-driver-535");
        assert_eq!(drivers[0].device, "GA106 [GeForce RTX 3060]");
        assert!(!drivers[0].free);
    }

    #[test]
    fn removal_keeps_running_and_newest() {
        let entries: Vec<KernelEntry> = ["6.8.0-31", "6.8.0-40", "6.8.0-45", "6.8.0-50"]
            .iter()
            .map(|version| KernelEntry {
                version: version.to_string(),
                package: format!("linux-image-{version}-generic"),
                running: *version == "6.8.0-45",
            })
            .collect();
        let candidates = removal_candidates(&entries);
        assert_eq!(
            candidates,
            vec![
                "linux-image-6.8.0-40-generic".to_string(),
                "linux-image-6.8.0-31-generic".to_string()
            ]
        );
    }

    #[test]
    fn refuses_to_remove_running_kernel() {
        let running = running_kernel();
        if running.is_empty() {
            return;
        }
        let result = remove_kernels(&[format!("linux-image-{running}")]);
        assert!(result.is_err());
    }
}
