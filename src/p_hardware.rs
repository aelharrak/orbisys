//! Matériel : Bluetooth et USB, énergie et batterie, ventilateurs et capteurs.

use crate::exec;
use crate::panel::*;
use std::path::Path;

// ---------------------------------------------------------------------------
// Bluetooth et USB
// ---------------------------------------------------------------------------

/// Adresse Bluetooth `AA:BB:CC:DD:EE:FF`.
pub fn valid_mac(text: &str) -> bool {
    let parts: Vec<&str> = text.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|ch| ch.is_ascii_hexdigit()))
}

/// Analyse `bluetoothctl devices` : `(adresse, nom)`.
pub fn parse_bt_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ').unwrap_or((rest, ""));
            valid_mac(mac).then(|| (mac.to_string(), name.trim().to_string()))
        })
        .collect()
}

/// `Connected: yes` dans `bluetoothctl info`.
pub fn bt_connected(info: &str) -> bool {
    info.lines().any(|line| line.trim() == "Connected: yes")
}

/// Un périphérique USB.
#[derive(Debug, Clone, PartialEq)]
pub struct UsbDevice {
    pub bus: String,
    pub id: String,
    pub name: String,
}

/// Analyse `lsusb`.
pub fn parse_lsusb(text: &str) -> Vec<UsbDevice> {
    text.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("Bus ")?;
            let (bus, rest) = rest.split_once(' ')?;
            let (_, rest) = rest.split_once(": ID ")?;
            let (id, name) = rest.split_once(' ').unwrap_or((rest, ""));
            Some(UsbDevice {
                bus: bus.to_string(),
                id: id.to_string(),
                name: name.trim().to_string(),
            })
        })
        .collect()
}

pub fn bluetooth_usb() -> Panel {
    let mut panel = Panel::default();
    if exec::exists("bluetoothctl") {
        let paired = parse_bt_devices(&run_lossy("bluetoothctl", &["devices", "Paired"]));
        let rows = paired
            .iter()
            .map(|(mac, name)| {
                let connected = bt_connected(&run_lossy("bluetoothctl", &["info", mac]));
                Row::owned(vec![
                    name.clone(),
                    mac.clone(),
                    if connected { "connecté" } else { "appairé" }.to_string(),
                ])
                .action(if connected {
                    Cmd::new("Déconnecter", "bluetoothctl", &["disconnect", mac])
                } else {
                    Cmd::new("Connecter", "bluetoothctl", &["connect", mac])
                })
            })
            .collect::<Vec<_>>();
        panel.metric("Appareils Bluetooth", rows.len().to_string(), Level::Info);
        panel.section(
            Section::new("Bluetooth (appairés)")
                .columns(&["Appareil", "Adresse", "État"])
                .rows(rows),
        );
    } else {
        panel.error("bluetoothctl absent : installez le paquet « bluez » pour gérer le Bluetooth.");
    }
    let usb = parse_lsusb(&run_lossy("lsusb", &[]));
    panel.metric("Périphériques USB", usb.len().to_string(), Level::Info);
    let rows = usb
        .iter()
        .map(|device| Row::new(&[&device.name, &device.id, &device.bus]))
        .collect();
    panel.section(
        Section::new("USB connectés (lecture seule)")
            .columns(&["Périphérique", "Identifiant", "Bus"])
            .rows(rows),
    );
    panel
}

// ---------------------------------------------------------------------------
// Énergie et batterie
// ---------------------------------------------------------------------------

/// Santé de la batterie en pourcentage de la capacité d'origine.
pub fn battery_health(full: u64, design: u64) -> Option<f32> {
    (design > 0).then(|| full as f32 * 100.0 / design as f32)
}

fn sys(path: &Path, file: &str) -> String {
    std::fs::read_to_string(path.join(file)).unwrap_or_default().trim().to_string()
}

pub fn power() -> Panel {
    let mut panel = Panel::default();
    let mut rows = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let kind = sys(&path, "type");
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if kind == "Battery" {
                let capacity: f32 = sys(&path, "capacity").parse().unwrap_or(0.0);
                let full = sys(&path, "energy_full").parse().or_else(|_| sys(&path, "charge_full").parse()).unwrap_or(0);
                let design = sys(&path, "energy_full_design").parse().or_else(|_| sys(&path, "charge_full_design").parse()).unwrap_or(0);
                let health = battery_health(full, design);
                let cycles = sys(&path, "cycle_count");
                panel.metric(&format!("Batterie {name}"), format!("{capacity:.0} % · {}", sys(&path, "status")), level_for_percent(100.0 - capacity, 70.0, 90.0));
                if let Some(health) = health {
                    panel.metric("Santé batterie", format!("{health:.0} %"), if health < 60.0 { Level::Bad } else if health < 80.0 { Level::Warn } else { Level::Ok });
                }
                rows.push(Row::owned(vec![
                    name,
                    format!("{capacity:.0} %"),
                    sys(&path, "status"),
                    if cycles.is_empty() || cycles == "0" { "—".to_string() } else { cycles },
                    sys(&path, "technology"),
                ]));
            } else if kind == "Mains" {
                let online = sys(&path, "online") == "1";
                panel.metric("Secteur", if online { "branché" } else { "débranché" }, Level::Info);
            }
        }
    }
    if rows.is_empty() {
        panel.metric("Batterie", "aucune détectée", Level::Info);
    } else {
        panel.section(
            Section::new("Batteries")
                .columns(&["Batterie", "Charge", "État", "Cycles", "Technologie"])
                .rows(rows),
        );
    }
    if exec::exists("powerprofilesctl") {
        let current = run_lossy("powerprofilesctl", &["get"]).trim().to_string();
        panel.metric("Profil d'alimentation", current.clone(), Level::Info);
        let mut section = Section::new("Profil d'alimentation");
        for profile in ["power-saver", "balanced", "performance"] {
            if profile != current {
                section = section.action(Cmd::new(profile, "powerprofilesctl", &["set", profile]));
            }
        }
        panel.section(section);
    }
    panel
}

// ---------------------------------------------------------------------------
// Ventilateurs et capteurs
// ---------------------------------------------------------------------------

/// Un capteur lu par lm-sensors.
#[derive(Debug, Clone, PartialEq)]
pub struct Sensor {
    pub chip: String,
    pub label: String,
    pub value: String,
}

/// Analyse la sortie de `sensors` (températures et ventilateurs).
pub fn parse_sensors(text: &str) -> Vec<Sensor> {
    let mut chip = String::new();
    let mut sensors = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if !line.contains(':') && !line.starts_with(' ') {
            chip = line.trim().to_string();
            continue;
        }
        let Some((label, rest)) = line.split_once(':') else {
            continue;
        };
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        let value = match tokens.as_slice() {
            [first, ..] if first.contains("°C") => first.to_string(),
            [number, unit, ..] if unit.eq_ignore_ascii_case("rpm") => format!("{number} RPM"),
            _ => continue,
        };
        sensors.push(Sensor {
            chip: chip.clone(),
            label: label.trim().to_string(),
            value,
        });
    }
    sensors
}

pub fn fans_sensors() -> Panel {
    let mut panel = Panel::default();
    let mut sensors = if exec::exists("sensors") {
        parse_sensors(&run_lossy("sensors", &[]))
    } else {
        Vec::new()
    };
    if sensors.is_empty() {
        // Repli sur hwmon : valeurs brutes du noyau.
        if let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") {
            for entry in entries.flatten() {
                let path = entry.path();
                let chip = sys(&path, "name");
                if let Ok(files) = std::fs::read_dir(&path) {
                    for file in files.flatten() {
                        let name = file.file_name().to_string_lossy().to_string();
                        let value: i64 = sys(&path, &name).parse().unwrap_or(i64::MIN);
                        if value == i64::MIN {
                            continue;
                        }
                        if name.starts_with("temp") && name.ends_with("_input") {
                            sensors.push(Sensor { chip: chip.clone(), label: name.replace("_input", ""), value: format!("{:.1} °C", value as f32 / 1000.0) });
                        } else if name.starts_with("fan") && name.ends_with("_input") {
                            sensors.push(Sensor { chip: chip.clone(), label: name.replace("_input", ""), value: format!("{value} RPM") });
                        }
                    }
                }
            }
        }
        if exec::exists("sensors") {
            panel.error("Aucun capteur renvoyé par lm-sensors : lancez « sensors-detect » pour les détecter.");
        } else {
            panel.error("lm-sensors absent : lecture directe de /sys/class/hwmon.");
        }
    }
    let fans: Vec<&Sensor> = sensors.iter().filter(|s| s.value.to_ascii_lowercase().contains("rpm")).collect();
    let temps: Vec<&Sensor> = sensors.iter().filter(|s| s.value.contains("°C")).collect();
    panel.metric("Ventilateurs", fans.len().to_string(), Level::Info);
    panel.metric("Sondes de température", temps.len().to_string(), Level::Info);
    let to_rows = |list: &[&Sensor]| -> Vec<Row> {
        list.iter()
            .map(|s| Row::new(&[&s.chip, &s.label, &s.value]))
            .collect()
    };
    panel.section(
        Section::new("Ventilateurs")
            .columns(&["Puce", "Ventilateur", "Vitesse"])
            .rows(to_rows(&fans)),
    );
    panel.section(
        Section::new("Températures")
            .columns(&["Puce", "Sonde", "Valeur"])
            .rows(to_rows(&temps))
            .note("Lecture seule."),
    );
    panel
}

// ---------------------------------------------------------------------------
// Firmware (fwupd)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct FwDevice {
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub updatable: bool,
}

/// Analyse `fwupdmgr get-devices --json`.
pub fn parse_fwupd_devices(json: &str) -> Vec<FwDevice> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value["Devices"]
        .as_array()
        .map(|devices| {
            devices
                .iter()
                .filter_map(|device| {
                    let name = device["Name"].as_str()?.to_string();
                    Some(FwDevice {
                        name,
                        vendor: device["Vendor"].as_str().unwrap_or("").to_string(),
                        version: device["Version"].as_str().unwrap_or("").to_string(),
                        updatable: device["Flags"]
                            .as_array()
                            .is_some_and(|flags| flags.iter().any(|flag| flag == "updatable")),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Analyse `fwupdmgr get-updates --json` : `(appareil, version actuelle, version proposée)`.
pub fn parse_fwupd_updates(json: &str) -> Vec<(String, String, String)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value["Devices"]
        .as_array()
        .map(|devices| {
            devices
                .iter()
                .filter_map(|device| {
                    let release = device["Releases"].as_array()?.first()?;
                    Some((
                        device["Name"].as_str()?.to_string(),
                        device["Version"].as_str().unwrap_or("").to_string(),
                        release["Version"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn firmware() -> Panel {
    let mut panel = Panel::default();
    if !exec::exists("fwupdmgr") {
        panel.error("fwupdmgr absent : installez le paquet « fwupd » pour gérer les mises à jour de firmware.");
        return panel;
    }
    let devices = parse_fwupd_devices(&run_lossy("fwupdmgr", &["get-devices", "--json"]));
    let updates = parse_fwupd_updates(&run_lossy("fwupdmgr", &["get-updates", "--json", "--no-unreported-check"]));
    panel.metric("Appareils", devices.len().to_string(), Level::Info);
    panel.metric("Mises à jour", updates.len().to_string(), if updates.is_empty() { Level::Ok } else { Level::Warn });
    let rows = updates
        .iter()
        .map(|(name, current, available)| Row::new(&[name, current, available]).bad(true))
        .collect();
    let mut section = Section::new("Mises à jour disponibles")
        .columns(&["Appareil", "Version actuelle", "Version proposée"])
        .rows(rows)
        .action(Cmd::new("Rafraîchir les métadonnées", "fwupdmgr", &["refresh", "--force"]));
    if !updates.is_empty() {
        section = section.action(
            Cmd::new("Installer les mises à jour", "fwupdmgr", &["update", "--assume-yes", "--no-reboot-check"])
                .confirm("Installer les mises à jour de firmware ? Un redémarrage peut être nécessaire et l'alimentation ne doit pas être coupée."),
        );
    }
    panel.section(section);
    let rows = devices
        .iter()
        .map(|device| {
            Row::new(&[&device.name, &device.vendor, &device.version, if device.updatable { "oui" } else { "non" }])
        })
        .collect();
    panel.section(
        Section::new("Appareils reconnus")
            .columns(&["Appareil", "Fabricant", "Version", "Mise à jour possible"])
            .rows(rows),
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_mac_addresses() {
        assert!(valid_mac("AA:BB:CC:DD:EE:FF"));
        assert!(valid_mac("00:1a:7d:da:71:13"));
        assert!(!valid_mac("AA:BB:CC:DD:EE"));
        assert!(!valid_mac("AA:BB:CC:DD:EE:GG"));
        assert!(!valid_mac("--help"));
    }

    #[test]
    fn parses_bluetooth() {
        let text = "Device AA:BB:CC:DD:EE:FF Casque Sony\nDevice zz Mauvais\nautre ligne\n";
        let devices = parse_bt_devices(text);
        assert_eq!(devices, vec![("AA:BB:CC:DD:EE:FF".to_string(), "Casque Sony".to_string())]);
        assert!(bt_connected("Device X\n\tConnected: yes\n"));
        assert!(!bt_connected("Connected: no"));
    }

    #[test]
    fn parses_lsusb() {
        let text = "Bus 001 Device 002: ID 8087:0024 Intel Corp. Hub\nBus 002 Device 001: ID 1d6b:0003 Linux Foundation 3.0 root hub\nbruit\n";
        let devices = parse_lsusb(text);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "8087:0024");
        assert_eq!(devices[0].name, "Intel Corp. Hub");
        assert_eq!(devices[1].bus, "002");
    }

    #[test]
    fn computes_battery_health() {
        assert_eq!(battery_health(45, 50), Some(90.0));
        assert_eq!(battery_health(10, 0), None);
    }

    #[test]
    fn parses_sensors() {
        let text = "coretemp-isa-0000\nAdapter: ISA adapter\nPackage id 0:  +52.0°C  (high = +100.0°C)\nCore 0:        +48.0°C  (high = +100.0°C)\n\nthinkpad-isa-0000\nAdapter: ISA adapter\nfan1:        2512 RPM\n";
        let sensors = parse_sensors(text);
        assert_eq!(sensors.len(), 3);
        assert_eq!(sensors[0].chip, "coretemp-isa-0000");
        assert_eq!(sensors[0].value, "+52.0°C");
        assert_eq!(sensors[2].value, "2512 RPM");
        assert_eq!(sensors[2].chip, "thinkpad-isa-0000");
    }

    #[test]
    fn parses_fwupd_json() {
        let devices = r#"{"Devices":[{"Name":"UEFI dbx","Vendor":"UEFI:Microsoft","Version":"77","Flags":["internal","updatable"]},{"Name":"SSD","Version":"1.2"}]}"#;
        let parsed = parse_fwupd_devices(devices);
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].updatable);
        assert!(!parsed[1].updatable);
        assert_eq!(parsed[1].vendor, "");
        let updates = r#"{"Devices":[{"Name":"UEFI dbx","Version":"77","Releases":[{"Version":"371"}]},{"Name":"Sans release","Releases":[]}]}"#;
        assert_eq!(parse_fwupd_updates(updates), vec![("UEFI dbx".to_string(), "77".to_string(), "371".to_string())]);
        assert!(parse_fwupd_devices("pas du json").is_empty());
        assert!(parse_fwupd_updates("{}").is_empty());
    }
}
