//! LVM et chiffrement LUKS : groupes de volumes, volumes logiques,
//! périphériques chiffrés, montage et fermeture.

use crate::exec;
use crate::util;
use serde::Serialize;
use serde_json::Value;

/// Un groupe de volumes LVM.
#[derive(Debug, Clone, Serialize)]
pub struct VolumeGroup {
    pub name: String,
    pub size: u64,
    pub free: u64,
    pub physical: usize,
    pub logical: usize,
}

/// Un volume logique LVM.
#[derive(Debug, Clone, Serialize)]
pub struct LogicalVolume {
    pub name: String,
    pub group: String,
    pub size: u64,
    pub attributes: String,
}

/// Un périphérique chiffré (LUKS) ouvert.
#[derive(Debug, Clone, Serialize)]
pub struct CryptDevice {
    pub name: String,
    pub device: String,
    pub fstype: String,
    pub mounted: bool,
}

/// État de la page Volumes & chiffrement.
#[derive(Debug, Clone, Default, Serialize)]
pub struct VolumesReport {
    pub groups: Vec<VolumeGroup>,
    pub volumes: Vec<LogicalVolume>,
    pub crypts: Vec<CryptDevice>,
    pub available: bool,
}

/// Un nom de volume LVM valide.
pub fn valid_volume_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '_' | '.' | '-'))
}

/// Une taille LVM valide (`10G`, `512M`, `+100%FREE` refusé).
pub fn valid_size(size: &str) -> bool {
    let trimmed = size.trim();
    if trimmed.is_empty() || trimmed.len() > 12 {
        return false;
    }
    let (digits, unit) = trimmed.split_at(trimmed.len() - 1);
    if !digits.chars().all(|c| c.is_ascii_digit()) || digits.is_empty() {
        return false;
    }
    matches!(unit, "K" | "M" | "G" | "T" | "k" | "m" | "g" | "t")
}

/// Analyse `lvs --noheadings --units b --nosuffix`.
pub fn parse_lvs(text: &str) -> Vec<LogicalVolume> {
    let mut volumes = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        let size = fields[2].trim_end_matches('B').parse().unwrap_or(0);
        volumes.push(LogicalVolume {
            name: fields[0].to_string(),
            group: fields[1].to_string(),
            size,
            attributes: fields[3].to_string(),
        });
    }
    volumes.sort_by(|a, b| (&a.group, &a.name).cmp(&(&b.group, &b.name)));
    volumes
}

/// Analyse `vgs --noheadings --units b --nosuffix`.
pub fn parse_vgs(text: &str) -> Vec<VolumeGroup> {
    let mut groups = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        groups.push(VolumeGroup {
            name: fields[0].to_string(),
            size: fields[1].trim_end_matches('B').parse().unwrap_or(0),
            free: fields[2].trim_end_matches('B').parse().unwrap_or(0),
            physical: fields[3].parse().unwrap_or(0),
            logical: fields[4].parse().unwrap_or(0),
        });
    }
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    groups
}

/// Extrait les périphériques chiffrés d'une sortie `lsblk -J`.
pub fn parse_lsblk_json(text: &str) -> Vec<CryptDevice> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let mut devices = Vec::new();
    if let Some(items) = value.get("blockdevices").and_then(|value| value.as_array()) {
        for item in items {
            walk(item, &mut devices);
        }
    }
    devices
}

fn walk(item: &Value, out: &mut Vec<CryptDevice>) {
    let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
    if kind == "crypt" {
        out.push(CryptDevice {
            name: item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            device: item
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            fstype: item
                .get("fstype")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mounted: item
                .get("mountpoint")
                .and_then(Value::as_str)
                .map(|value| !value.is_empty())
                .unwrap_or(false),
        });
    }
    if let Some(children) = item.get("children").and_then(Value::as_array) {
        for child in children {
            walk(child, out);
        }
    }
}

fn lvm_output(program: &str, fields: &str) -> String {
    if !exec::exists(program) {
        return String::new();
    }
    exec::run(
        program,
        &["--noheadings", "--units", "b", "--nosuffix", "-o", fields],
    )
    .map(|captured| captured.stdout)
    .unwrap_or_default()
}

/// Lit l'état LVM et les périphériques chiffrés.
pub fn report() -> VolumesReport {
    let available = exec::exists("lvs");
    let groups = parse_vgs(&lvm_output("vgs", "vg_name,vg_size,vg_free,pv_count,lv_count"));
    let volumes = parse_lvs(&lvm_output("lvs", "lv_name,vg_name,lv_size,lv_attr"));
    let crypts = exec::run(
        "lsblk",
        &[
            "-J",
            "-o",
            "NAME,PATH,TYPE,FSTYPE,MOUNTPOINT",
        ],
    )
    .map(|captured| parse_lsblk_json(&captured.stdout))
    .unwrap_or_default();
    VolumesReport {
        groups,
        volumes,
        crypts,
        available,
    }
}

/// Crée un volume logique.
pub fn create_volume(group: &str, name: &str, size: &str) -> Result<String, String> {
    if !valid_volume_name(group) || !valid_volume_name(name) {
        return Err("nom de groupe ou de volume invalide".to_string());
    }
    if !valid_size(size) {
        return Err("taille invalide : attendu un nombre suivi de K, M, G ou T".to_string());
    }
    exec::run_privileged(
        "lvcreate",
        &[
            "-L".to_string(),
            size.to_string(),
            "-n".to_string(),
            name.to_string(),
            group.to_string(),
        ],
    )?;
    Ok(format!("volume {group}/{name} créé ({size})"))
}

/// Supprime un volume logique (`/dev/vg/lv`).
pub fn remove_volume(reference: &str) -> Result<String, String> {
    let Some((group, name)) = reference.split_once('/') else {
        return Err("référence attendue : groupe/volume".to_string());
    };
    if !valid_volume_name(group) || !valid_volume_name(name) {
        return Err("référence de volume invalide".to_string());
    }
    exec::run_privileged(
        "lvremove",
        &["-f".to_string(), format!("{group}/{name}")],
    )?;
    Ok(format!("volume {group}/{name} supprimé"))
}

/// Ferme un périphérique chiffré.
pub fn close_crypt(name: &str) -> Result<String, String> {
    if !valid_volume_name(name) {
        return Err("nom de périphérique invalide".to_string());
    }
    exec::run_privileged("cryptsetup", &["close".to_string(), name.to_string()])?;
    Ok(format!("{name} fermé"))
}

/// Monte ou démonte un périphérique via udisks (sans privilège root).
pub fn mount_device(device: &str, mount: bool) -> Result<String, String> {
    if !device.starts_with("/dev/") || !util::valid_app_id(device.trim_start_matches("/dev/")) {
        return Err("périphérique invalide".to_string());
    }
    let action = if mount { "mount" } else { "unmount" };
    let captured = exec::run(
        "udisksctl",
        &[action, "-b", device],
    )
    .map_err(|error| error.0)?;
    if captured.ok {
        Ok(captured.text())
    } else {
        Err(captured.error_text())
    }
}

/// Taille totale des volumes d'un groupe (pour l'affichage).
pub fn group_usage(group: &VolumeGroup) -> f32 {
    if group.size == 0 {
        return 0.0;
    }
    ((group.size - group.free) as f64 / group.size as f64 * 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_names_are_validated() {
        assert!(valid_volume_name("home"));
        assert!(valid_volume_name("data-lv"));
        assert!(!valid_volume_name(""));
        assert!(!valid_volume_name("-rf"));
        assert!(!valid_volume_name("a;b"));
    }

    #[test]
    fn sizes_are_validated() {
        assert!(valid_size("10G"));
        assert!(valid_size("512M"));
        assert!(valid_size("1T"));
        assert!(!valid_size("10"));
        assert!(!valid_size("+100%FREE"));
        assert!(!valid_size("10 GB"));
        assert!(!valid_size("abc"));
    }

    #[test]
    fn parses_lvs_output() {
        let text = "  root ubuntu-vg 21474836480 -wi-ao----\n  home ubuntu-vg 322122547200 -wi-ao----\n";
        let volumes = parse_lvs(text);
        assert_eq!(volumes.len(), 2);
        assert_eq!(volumes[0].name, "home");
        assert_eq!(volumes[0].group, "ubuntu-vg");
        assert_eq!(volumes[1].size, 21_474_836_480);
        assert_eq!(volumes[1].attributes, "-wi-ao----");
    }

    #[test]
    fn parses_vgs_output() {
        let text = "  ubuntu-vg 500107862016 268435456000 1 3\n";
        let groups = parse_vgs(text);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].physical, 1);
        assert_eq!(groups[0].logical, 3);
        assert!(group_usage(&groups[0]) > 40.0);
    }

    #[test]
    fn finds_crypt_devices_in_lsblk() {
        let text = r#"{"blockdevices":[
            {"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","fstype":null,"mountpoint":null,
             "children":[
               {"name":"nvme0n1p3","path":"/dev/nvme0n1p3","type":"part","fstype":"crypto_LUKS","mountpoint":null,
                "children":[
                  {"name":"cryptroot","path":"/dev/mapper/cryptroot","type":"crypt","fstype":"ext4","mountpoint":"/"}
                ]}
             ]}
        ]}"#;
        let devices = parse_lsblk_json(text);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "cryptroot");
        assert!(devices[0].mounted);
        assert_eq!(devices[0].fstype, "ext4");
    }

    #[test]
    fn rejects_bad_volume_references() {
        assert!(remove_volume("/dev/vg/lv").is_err());
        assert!(remove_volume("groupe").is_err());
        assert!(close_crypt("a;reboot").is_err());
        assert!(mount_device("/dev/sda1; rm -rf /", true).is_err());
    }
}
