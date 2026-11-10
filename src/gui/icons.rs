//! Icônes linéaires Lucide (licence ISC, assets/icons/LICENSE), police intégrée.
//! Généré à partir de assets/icons/info.json.

pub const FONT: &[u8] = include_bytes!("../../assets/icons/lucide.ttf");

/// (suffixe de la clé `page.<suffixe>`, glyphe)
const PAGES: &[(&str, char)] = &[
    ("dashboard", '\u{e1c0}'), // layout-dashboard
    ("processes", '\u{e10b}'), // list
    ("monitor", '\u{e038}'), // activity
    ("alerts", '\u{e05d}'), // bell
    ("health", '\u{e371}'), // heart-pulse
    ("reports", '\u{e0d0}'), // file-text
    ("history", '\u{e1f4}'), // history
    ("delivery", '\u{e155}'), // send
    ("schedule", '\u{e303}'), // calendar-clock
    ("software", '\u{e12c}'), // package
    ("updates", '\u{e148}'), // refresh-cw
    ("aptrepos", '\u{e0b1}'), // database
    ("advpackages", '\u{e267}'), // package-plus
    ("runtimes", '\u{e184}'), // terminal
    ("containers", '\u{e4d9}'), // container
    ("containersadv", '\u{e2cf}'), // boxes
    ("disk", '\u{e0f0}'), // hard-drive
    ("storage", '\u{e156}'), // server
    ("quotas", '\u{e1be}'), // gauge
    ("duplicates", '\u{e0a2}'), // copy
    ("cleanup", '\u{e18d}'), // trash-2
    ("advcleanup", '\u{e28e}'), // eraser
    ("mounts", '\u{e33f}'), // folder-tree
    ("usbboot", '\u{e359}'), // usb
    ("archives", '\u{e045}'), // archive
    ("backup", '\u{e150}'), // save
    ("cloudbackup", '\u{e095}'), // cloud-upload
    ("kernel", '\u{e0ad}'), // cpu
    ("volumes", '\u{e52d}'), // layers
    ("peripherals", '\u{e382}'), // plug
    ("printers", '\u{e144}'), // printer
    ("desktop", '\u{e120}'), // monitor
    ("automation", '\u{e429}'), // workflow
    ("about", '\u{e0fe}'), // info
    ("acl", '\u{e535}'), // lock-keyhole
    ("actionlog", '\u{e463}'), // scroll-text
    ("benchmark", '\u{e1df}'), // timer
    ("bluetooth", '\u{e060}'), // bluetooth
    ("btrfs", '\u{e0e5}'), // git-branch
    ("btrfsmaint", '\u{e1b0}'), // wrench
    ("certs", '\u{e240}'), // badge-check
    ("cis", '\u{e1fe}'), // shield-check
    ("customunits", '\u{e29b}'), // puzzle
    ("dotfiles", '\u{e0c7}'), // file-code
    ("fans", '\u{e37c}'), // fan
    ("firewall", '\u{e0d7}'), // flame
    ("firmware", '\u{e61e}'), // microchip
    ("fonts", '\u{e197}'), // type
    ("hardware", '\u{e0ad}'), // cpu
    ("integrity", '\u{e2ca}'), // fingerprint
    ("klogs", '\u{e328}'), // file-terminal
    ("lan", '\u{e128}'), // network
    ("locale", '\u{e103}'), // languages
    ("logadvanced", '\u{e0cf}'), // file-search
    ("logs", '\u{e463}'), // scroll-text
    ("maintenance", '\u{e1b0}'), // wrench
    ("netadvanced", '\u{e128}'), // network
    ("netmounts", '\u{e33e}'), // folder-symlink
    ("network", '\u{e128}'), // network
    ("optimize", '\u{e1b3}'), // zap
    ("orphans", '\u{e19b}'), // unlink
    ("perf", '\u{e1be}'), // gauge
    ("power", '\u{e058}'), // battery-charging
    ("profiles", '\u{e345}'), // user-cog
    ("scan", '\u{e49b}'), // radar
    ("secureboot", '\u{e15b}'), // shield
    ("security", '\u{e51b}'), // shield-half
    ("services", '\u{e157}'), // settings
    ("sessions", '\u{e1a3}'), // users
    ("snapshots", '\u{e068}'), // camera
    ("sshgpg", '\u{e4a7}'), // key-round
    ("startup", '\u{e285}'), // rocket
    ("timers", '\u{e03a}'), // alarm-clock
    ("traffic", '\u{e04a}'), // arrow-down-up
    ("troubleshoot", '\u{e106}'), // life-buoy
    ("users", '\u{e1a3}'), // users
    ("wifivpn", '\u{e1ad}'), // wifi
    ("apparmor", '\u{e1fd}'), // shield-alert
    ("cpu", '\u{e0ad}'), // cpu
    ("memory", '\u{e0b1}'), // database
    ("dot", '\u{e348}'), // circle-dot
];

/// Glyphe d'une page d'après sa clé de traduction (`page.processes`).
pub fn for_page_key(key: &str) -> char {
    let name = key.strip_prefix("page.").unwrap_or(key);
    PAGES.iter().find(|(k, _)| *k == name).map(|(_, c)| *c).unwrap_or('\u{e348}')
}

pub const CPU: char = '\u{e0ad}';
pub const DATABASE: char = '\u{e0b1}';
pub const HARD_DRIVE: char = '\u{e0f0}';
pub const NETWORK: char = '\u{e128}';
#[allow(dead_code)]
pub const ACTIVITY: char = '\u{e038}';
#[allow(dead_code)]
pub const HEART_PULSE: char = '\u{e371}';
#[allow(dead_code)]
pub const SETTINGS: char = '\u{e157}';

/// Police d'icônes à la taille demandée.
pub fn font(size: f32) -> eframe::egui::FontId {
    eframe::egui::FontId::new(size, eframe::egui::FontFamily::Name("icons".into()))
}
#[allow(dead_code)]
pub const SWAP: char = '\u{e249}';
pub const LIST: char = '\u{e10b}';
