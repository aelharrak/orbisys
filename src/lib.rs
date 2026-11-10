//! OrbiSys — noyau système.
//!
//! Toute la logique d'administration vit ici : elle est indépendante de
//! l'interface, testable, et partagée par le mode graphique et le mode CLI.
//!
//! Règles héritées de la version Python et conservées :
//! - aucune commande n'est passée par un shell (`Command` avec liste d'arguments) ;
//! - les noms de paquets, d'unités et les chemins sont validés avant usage ;
//! - les actions privilégiées passent par Polkit (`pkexec`) ;
//! - les écrans de lecture ne modifient rien.

pub mod arabic;
pub mod automation;
pub mod backup;
pub mod btrfs;
pub mod cis;
pub mod cleanup;
pub mod cli;
pub mod desktop;
pub mod disk;
pub mod cups;
pub mod distro;
pub mod exec;
pub mod firewall;
pub mod fonts;
pub mod hardware;
pub mod i18n;
pub mod i18n_ui;
pub mod inventory;
pub mod journal;
pub mod p_monitor;
pub mod p_software;
pub mod p_system;
pub mod p_storage;
pub mod p_network;
pub mod p_scan;
pub mod p_security;
pub mod p_hardware;
pub mod p_backup;
pub mod panel;
pub mod panels;
pub mod kernel;
pub mod klogs;
pub mod logs;
pub mod locale;
pub mod logsearch;
pub mod netadv;
pub mod netmounts;
pub mod units;
pub mod lvm;
pub mod manual;
pub mod network;
pub mod peripherals;
pub mod optimize;
pub mod perf;
pub mod pkg;
pub mod processes;
pub mod orphans;
pub mod quota;
pub mod report;
pub mod security;
pub mod services;
pub mod sessions;
pub mod snapshots;
pub mod storage;
pub mod timers;
pub mod util;

/// Version affichée par l'interface et le mode CLI.
pub const APP_VERSION: &str = "1.0.0";
