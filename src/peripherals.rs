//! Périphériques : files d'impression CUPS et audio PulseAudio/PipeWire.

use crate::exec;
use serde::Serialize;

/// Une imprimante CUPS.
#[derive(Debug, Clone, Serialize)]
pub struct Printer {
    pub name: String,
    pub state: String,
    pub enabled: bool,
    pub device: String,
    pub default: bool,
}

/// Une sortie ou entrée audio.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub name: String,
    pub description: String,
    pub kind: String,
    pub volume: u8,
    pub muted: bool,
    pub default: bool,
}

/// État de la page Imprimantes & audio.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PeripheralsReport {
    pub printers: Vec<Printer>,
    pub outputs: Vec<AudioDevice>,
    pub inputs: Vec<AudioDevice>,
    pub default_sink: String,
    pub cups_available: bool,
    pub audio_available: bool,
}

/// Nom d'imprimante ou de périphérique audio valide.
pub fn valid_device_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '+'))
}

/// Analyse `lpstat -p -d`.
pub fn parse_lpstat_printers(text: &str) -> (Vec<Printer>, String) {
    let mut printers = Vec::new();
    let mut default = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("system default destination:") {
            default = rest.trim().to_string();
            continue;
        }
        let Some(rest) = line.strip_prefix("printer ") else {
            continue;
        };
        let mut parts = rest.splitn(2, ' ');
        let name = parts.next().unwrap_or("").trim().to_string();
        if !valid_device_name(&name) {
            continue;
        }
        let details = parts.next().unwrap_or("");
        let enabled = !details.contains("disabled");
        let state = if details.contains("now printing") {
            "impression en cours"
        } else if !enabled {
            "désactivée"
        } else if details.contains("is idle") {
            "prête"
        } else {
            "inconnue"
        };
        printers.push(Printer {
            name,
            state: state.to_string(),
            enabled,
            device: String::new(),
            default: false,
        });
    }
    (printers, default)
}

/// Analyse `lpstat -v` (`device for nom: uri`).
pub fn parse_lpstat_devices(text: &str) -> Vec<(String, String)> {
    let mut devices = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_end_matches('.');
        let Some(rest) = line.strip_prefix("device for ") else {
            continue;
        };
        let Some((name, uri)) = rest.split_once(':') else {
            continue;
        };
        devices.push((name.trim().to_string(), uri.trim().to_string()));
    }
    devices
}

fn volume_percent(line: &str) -> u8 {
    line.split('/')
        .nth(1)
        .map(|value| {
            value
                .trim()
                .trim_end_matches('%')
                .trim()
                .parse()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// Analyse `pactl list sinks` ou `pactl list sources`.
pub fn parse_pactl_devices(text: &str, kind: &str) -> Vec<AudioDevice> {
    let mut devices: Vec<AudioDevice> = Vec::new();
    let mut current: Option<AudioDevice> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sink #") || trimmed.starts_with("Source #") {
            if let Some(device) = current.take() {
                devices.push(device);
            }
            current = Some(AudioDevice {
                name: String::new(),
                description: String::new(),
                kind: kind.to_string(),
                volume: 0,
                muted: false,
                default: false,
            });
            continue;
        }
        let Some(device) = current.as_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("Name: ") {
            device.name = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("Description: ") {
            device.description = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("Mute: ") {
            device.muted = value.trim().eq_ignore_ascii_case("yes");
        } else if trimmed.starts_with("Volume:") && device.volume == 0 {
            device.volume = volume_percent(trimmed);
        }
    }
    if let Some(device) = current {
        devices.push(device);
    }
    devices.retain(|device| !device.name.is_empty());
    devices
}

/// Lit l'état des imprimantes et de l'audio.
pub fn report() -> PeripheralsReport {
    let cups_available = exec::exists("lpstat");
    let mut printers = Vec::new();
    let mut default_printer = String::new();
    if cups_available {
        if let Ok(captured) = exec::run("lpstat", &["-p", "-d"]) {
            let (found, default) = parse_lpstat_printers(&captured.stdout);
            printers = found;
            default_printer = default;
        }
        if let Ok(captured) = exec::run("lpstat", &["-v"]) {
            for (name, uri) in parse_lpstat_devices(&captured.stdout) {
                if let Some(printer) = printers.iter_mut().find(|entry| entry.name == name) {
                    printer.device = uri;
                }
            }
        }
        for printer in &mut printers {
            printer.default = !default_printer.is_empty() && printer.name == default_printer;
        }
    }

    let audio_available = exec::exists("pactl");
    let mut outputs = Vec::new();
    let mut inputs = Vec::new();
    let mut default_sink = String::new();
    if audio_available {
        if let Ok(captured) = exec::run("pactl", &["info"]) {
            for line in captured.stdout.lines() {
                if let Some(value) = line.trim().strip_prefix("Default Sink:") {
                    default_sink = value.trim().to_string();
                }
            }
        }
        if let Ok(captured) = exec::run("pactl", &["list", "sinks"]) {
            outputs = parse_pactl_devices(&captured.stdout, "sortie");
        }
        if let Ok(captured) = exec::run("pactl", &["list", "sources"]) {
            inputs = parse_pactl_devices(&captured.stdout, "entrée");
        }
    }

    PeripheralsReport {
        printers,
        outputs,
        inputs,
        default_sink,
        cups_available,
        audio_available,
    }
}

/// Active ou désactive une imprimante (`cupsenable`/`cupsdisable`).
pub fn set_printer_enabled(name: &str, enabled: bool) -> Result<String, String> {
    if !valid_device_name(name) {
        return Err("nom d'imprimante invalide".to_string());
    }
    let program = if enabled { "cupsenable" } else { "cupsdisable" };
    exec::run_privileged(program, &[name.to_string()])?;
    Ok(format!(
        "imprimante {name} {}",
        if enabled { "activée" } else { "désactivée" }
    ))
}

/// Définit l'imprimante par défaut.
pub fn set_default_printer(name: &str) -> Result<String, String> {
    if !valid_device_name(name) {
        return Err("nom d'imprimante invalide".to_string());
    }
    exec::run_privileged("lpadmin", &["-d".to_string(), name.to_string()])?;
    Ok(format!("imprimante par défaut : {name}"))
}

/// Supprime une imprimante et sa file.
pub fn remove_printer(name: &str) -> Result<String, String> {
    if !valid_device_name(name) {
        return Err("nom d'imprimante invalide".to_string());
    }
    exec::run_privileged("lpadmin", &["-x".to_string(), name.to_string()])?;
    Ok(format!("imprimante {name} supprimée"))
}

/// Définit la sortie audio par défaut.
pub fn set_default_sink(name: &str) -> Result<String, String> {
    if !valid_device_name(name) {
        return Err("nom de sortie audio invalide".to_string());
    }
    let captured = exec::run("pactl", &["set-default-sink", name]).map_err(|error| error.0)?;
    if captured.ok {
        Ok(format!("sortie par défaut : {name}"))
    } else {
        Err(captured.error_text())
    }
}

/// Active ou coupe le son d'une sortie.
pub fn toggle_mute(name: &str) -> Result<String, String> {
    if !valid_device_name(name) {
        return Err("nom de sortie audio invalide".to_string());
    }
    let captured =
        exec::run("pactl", &["set-sink-mute", name, "toggle"]).map_err(|error| error.0)?;
    if captured.ok {
        Ok(format!("sourdine basculée pour {name}"))
    } else {
        Err(captured.error_text())
    }
}

/// Ajuste le volume d'une sortie en pourcentage relatif.
pub fn adjust_volume(name: &str, delta: i32) -> Result<String, String> {
    if !valid_device_name(name) {
        return Err("nom de sortie audio invalide".to_string());
    }
    if !(-100..=100).contains(&delta) || delta == 0 {
        return Err("ajustement attendu entre -100 et +100".to_string());
    }
    let argument = format!("{}{}%", if delta > 0 { "+" } else { "" }, delta);
    let captured =
        exec::run("pactl", &["set-sink-volume", name, &argument]).map_err(|error| error.0)?;
    if captured.ok {
        Ok(format!("volume de {name} ajusté de {argument}"))
    } else {
        Err(captured.error_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_validated() {
        assert!(valid_device_name("HP_LaserJet-100"));
        assert!(!valid_device_name(""));
        assert!(!valid_device_name("-x"));
        assert!(!valid_device_name("a;reboot"));
    }

    #[test]
    fn parses_printers_and_default() {
        let text = "system default destination: HP_LaserJet\nprinter HP_LaserJet is idle.  enabled since Mon 02 Nov 2026\nprinter Bureau disabled since Tue 03 Nov 2026 -\n";
        let (printers, default) = parse_lpstat_printers(text);
        assert_eq!(default, "HP_LaserJet");
        assert_eq!(printers.len(), 2);
        assert!(printers[0].enabled);
        assert_eq!(printers[0].state, "prête");
        assert!(!printers[1].enabled);
        assert_eq!(printers[1].state, "désactivée");
    }

    #[test]
    fn parses_printer_devices() {
        let text = "device for HP_LaserJet: ipp://192.168.1.20/ipp/print\ndevice for PDF: cups-pdf:/\n";
        let devices = parse_lpstat_devices(text);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].0, "HP_LaserJet");
        assert_eq!(devices[0].1, "ipp://192.168.1.20/ipp/print");
    }

    #[test]
    fn parses_pactl_sinks() {
        let text = "Sink #0\n\tState: RUNNING\n\tName: alsa_output.pci-0000_00_1f.3.analog-stereo\n\tDescription: Audio interne Stéréo analogique\n\tMute: no\n\tVolume: front-left: 32768 /  50% / -18.06 dB,   front-right: 32768 /  50% / -18.06 dB\nSink #1\n\tState: SUSPENDED\n\tName: bluez_output.AC_12_2F\n\tDescription: Casque Bluetooth\n\tMute: yes\n\tVolume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB\n";
        let devices = parse_pactl_devices(text, "sortie");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].volume, 50);
        assert!(!devices[0].muted);
        assert_eq!(devices[1].volume, 100);
        assert!(devices[1].muted);
        assert_eq!(devices[1].description, "Casque Bluetooth");
    }

    #[test]
    fn parses_pactl_sources() {
        let text = "Source #0\n\tName: alsa_input.pci-0000_00_1f.3.analog-stereo\n\tDescription: Micro interne\n\tMute: no\n\tVolume: mono: 65536 / 100% / 0.00 dB\n";
        let devices = parse_pactl_devices(text, "entrée");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].volume, 100);
    }

    #[test]
    fn volume_adjustments_are_bounded() {
        assert!(adjust_volume("a;reboot", 10).is_err());
        assert!(adjust_volume("alsa_output.x", 0).is_err());
        assert!(adjust_volume("alsa_output.x", 200).is_err());
    }
}
