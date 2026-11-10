//! Réseau étendu : interfaces et adresses, routes, DNS, VPN, partages Samba
//! et diagnostics de connectivité.

use crate::exec;
use crate::util;
use serde::Serialize;
use std::path::Path;

/// Une interface réseau avec ses adresses IPv4.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Interface {
    pub name: String,
    pub state: String,
    pub mac: String,
    pub mtu: String,
    pub addresses: Vec<String>,
}

/// Résolution DNS déclarée dans `/etc/resolv.conf`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct DnsConfig {
    pub nameservers: Vec<String>,
    pub search: Vec<String>,
}

/// Une entrée de `/etc/hosts`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HostEntry {
    pub address: String,
    pub names: Vec<String>,
}

/// Partage Samba déclaré dans `/etc/samba/smb.conf`.
#[derive(Debug, Clone, Serialize)]
pub struct Share {
    pub name: String,
    pub path: String,
    pub read_only: bool,
    pub browseable: bool,
}

/// Diagnostic de connectivité.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Diagnostics {
    pub gateway: String,
    pub device: String,
    pub ping_output: String,
    pub dns_output: String,
    pub internet_output: String,
}

/// Données affichées par l'écran Réseau étendu.
#[derive(Debug, Clone, Default)]
pub struct NetAdvData {
    pub interfaces: Vec<Interface>,
    pub dns: DnsConfig,
    pub vpn: Vec<String>,
    pub shares: Vec<Share>,
}

/// Collecte interfaces, DNS, VPN et partages en une passe.
pub fn collect() -> NetAdvData {
    NetAdvData {
        interfaces: interfaces().unwrap_or_default(),
        dns: dns_config(),
        vpn: vpn_connections(),
        shares: samba_shares(),
    }
}

/// Hôte ou adresse acceptable pour `ping`/`getent`.
pub fn valid_host(host: &str) -> bool {
    let host = host.trim();
    if host.is_empty() || host.len() > 253 || host.starts_with('-') {
        return false;
    }
    host.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'))
        && !host.contains("..")
}

/// Analyse `ip -o link show`.
pub fn parse_links(text: &str) -> Vec<Interface> {
    let mut interfaces: Vec<Interface> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(index) = fields.next() else {
            continue;
        };
        if !index.trim_end_matches(':').chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Some(raw_name) = fields.next() else {
            continue;
        };
        let name = raw_name.trim_end_matches(':').to_string();
        if name.is_empty() || name == "lo" {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let value_after = |key: &str| -> String {
            tokens
                .iter()
                .position(|token| *token == key)
                .and_then(|index| tokens.get(index + 1))
                .map(|value| value.trim_end_matches(',').to_string())
                .unwrap_or_default()
        };
        interfaces.push(Interface {
            name,
            state: value_after("state"),
            mac: value_after("link/ether"),
            mtu: value_after("mtu"),
            addresses: Vec::new(),
        });
    }
    interfaces
}

/// Analyse `ip -o -4 addr show` et rattache les adresses aux interfaces.
pub fn parse_addrs(text: &str, interfaces: &mut [Interface]) {
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(_index) = fields.next() else {
            continue;
        };
        let Some(raw_name) = fields.next() else {
            continue;
        };
        let name = raw_name.trim_end_matches(':');
        let Some(position) = fields.position(|token| token == "inet") else {
            continue;
        };
        let _ = position;
        let rest: Vec<&str> = line.split_whitespace().collect();
        let inet_index = rest.iter().position(|token| *token == "inet");
        let Some(address) = inet_index.and_then(|index| rest.get(index + 1)) else {
            continue;
        };
        if let Some(interface) = interfaces.iter_mut().find(|item| item.name == name) {
            interface.addresses.push(address.to_string());
        }
    }
}

/// Analyse `ip route show default` : (passerelle, interface).
pub fn parse_default_route(text: &str) -> (String, String) {
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.first() != Some(&"default") {
            continue;
        }
        let gateway = tokens
            .iter()
            .position(|token| *token == "via")
            .and_then(|index| tokens.get(index + 1))
            .map(|value| value.to_string())
            .unwrap_or_default();
        let device = tokens
            .iter()
            .position(|token| *token == "dev")
            .and_then(|index| tokens.get(index + 1))
            .map(|value| value.to_string())
            .unwrap_or_default();
        if !gateway.is_empty() || !device.is_empty() {
            return (gateway, device);
        }
    }
    (String::new(), String::new())
}

/// Analyse `/etc/resolv.conf`.
pub fn parse_resolv(text: &str) -> DnsConfig {
    let mut config = DnsConfig::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("nameserver") => {
                if let Some(server) = fields.next() {
                    config.nameservers.push(server.to_string());
                }
            }
            Some("search") | Some("domain") => {
                for domain in fields {
                    config.search.push(domain.to_string());
                }
            }
            _ => {}
        }
    }
    config
}

/// Analyse `/etc/samba/smb.conf` (sections hors `[global]`).
pub fn parse_smb_conf(text: &str) -> Vec<Share> {
    let mut shares: Vec<Share> = Vec::new();
    let mut current: Option<Share> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if let Some(section) = trimmed.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            if let Some(share) = current.take() {
                shares.push(share);
            }
            if section.eq_ignore_ascii_case("global") || section.is_empty() {
                continue;
            }
            current = Some(Share {
                name: section.to_string(),
                path: String::new(),
                read_only: false,
                browseable: true,
            });
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        let Some(share) = current.as_mut() else {
            continue;
        };
        match key.as_str() {
            "path" => share.path = value.to_string(),
            "read only" | "readonly" => share.read_only = parse_bool(value, true),
            "writeable" | "writable" => share.read_only = !parse_bool(value, false),
            "browseable" | "browsable" => share.browseable = parse_bool(value, true),
            _ => {}
        }
    }
    if let Some(share) = current {
        shares.push(share);
    }
    shares
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" | "on" => true,
        "no" | "false" | "0" | "off" => false,
        _ => default,
    }
}

/// Extrait les connexions VPN de `nmcli -t`.
pub fn parse_nmcli_vpn(text: &str) -> Vec<String> {
    let mut connections: Vec<String> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.splitn(3, ':').collect();
        if fields.len() < 2 {
            continue;
        }
        let name = fields[0].trim();
        let kind = fields[1].trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if kind.contains("vpn") || kind.contains("wireguard") {
            if !connections.iter().any(|item| item == name) {
                connections.push(name.to_string());
            }
        }
    }
    connections
}

/// Interfaces WireGuard actives (`wg show interfaces`).
pub fn parse_wireguard(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// Liste les interfaces avec leurs adresses.
pub fn interfaces() -> Result<Vec<Interface>, String> {
    let links = exec::run_with_timeout(
        "ip",
        &["-o", "link", "show"],
        std::time::Duration::from_secs(15),
    )
    .map_err(|error| error.0)?;
    if !links.ok && links.stdout.trim().is_empty() {
        return Err(links.error_text());
    }
    let mut interfaces = parse_links(&links.stdout);
    if let Ok(addresses) = exec::run_with_timeout(
        "ip",
        &["-o", "-4", "addr", "show"],
        std::time::Duration::from_secs(15),
    ) {
        parse_addrs(&addresses.stdout, &mut interfaces);
    }
    Ok(interfaces)
}

/// Route par défaut.
pub fn default_route() -> (String, String) {
    exec::run_with_timeout(
        "ip",
        &["route", "show", "default"],
        std::time::Duration::from_secs(15),
    )
    .map(|captured| parse_default_route(&captured.stdout))
    .unwrap_or_default()
}

/// Configuration DNS courante.
pub fn dns_config() -> DnsConfig {
    std::fs::read_to_string("/etc/resolv.conf")
        .map(|text| parse_resolv(&text))
        .unwrap_or_default()
}

/// Connexions VPN connues de NetworkManager.
pub fn vpn_connections() -> Vec<String> {
    let mut connections = exec::run_with_timeout(
        "nmcli",
        &["-t", "-f", "NAME,TYPE", "connection", "show"],
        std::time::Duration::from_secs(20),
    )
    .map(|captured| parse_nmcli_vpn(&captured.stdout))
    .unwrap_or_default();
    if let Ok(captured) = exec::run_with_timeout(
        "wg",
        &["show", "interfaces"],
        std::time::Duration::from_secs(10),
    ) {
        for name in parse_wireguard(&captured.stdout) {
            if !connections.iter().any(|item| item == &name) {
                connections.push(format!("{name} (WireGuard)"));
            }
        }
    }
    connections
}

/// Partages Samba configurés.
pub fn samba_shares() -> Vec<Share> {
    std::fs::read_to_string("/etc/samba/smb.conf")
        .map(|text| parse_smb_conf(&text))
        .unwrap_or_default()
}

/// Teste la joignabilité d'un hôte (3 paquets).
pub fn ping(host: &str) -> Result<String, String> {
    if !valid_host(host) {
        return Err(format!("hôte refusé : {host}"));
    }
    let captured = exec::run_with_timeout(
        "ping",
        &["-n", "-c", "3", "-W", "2", host],
        std::time::Duration::from_secs(15),
    )
    .map_err(|error| error.0)?;
    if captured.stdout.trim().is_empty() {
        return Err(captured.error_text());
    }
    Ok(captured.text())
}

/// Résout un nom via `getent hosts`.
pub fn resolve(host: &str) -> Result<String, String> {
    if !valid_host(host) {
        return Err(format!("hôte refusé : {host}"));
    }
    let captured = exec::run("getent", &["hosts", host]).map_err(|error| error.0)?;
    if !captured.ok || captured.stdout.trim().is_empty() {
        return Err(format!("résolution impossible : {host}"));
    }
    Ok(captured.stdout)
}

/// Diagnostic complet : route par défaut, passerelle, DNS, Internet.
pub fn diagnose() -> Diagnostics {
    let (gateway, device) = default_route();
    let mut diagnostics = Diagnostics {
        gateway: gateway.clone(),
        device,
        ..Diagnostics::default()
    };
    if !gateway.is_empty() {
        diagnostics.ping_output = ping(&gateway).unwrap_or_else(|error| error);
    } else {
        diagnostics.ping_output = "aucune route par défaut".to_string();
    }
    diagnostics.dns_output = match resolve("fr.archive.ubuntu.com") {
        Ok(text) => text,
        Err(error) => error,
    };
    diagnostics.internet_output = match ping("1.1.1.1") {
        Ok(text) => text,
        Err(error) => error,
    };
    diagnostics
}

// ---------------------------------------------------------------------------
// Résolveurs DNS et /etc/hosts
// ---------------------------------------------------------------------------

/// Résolveurs proposés : (nom, serveurs).
pub const DNS_PRESETS: [(&str, [&str; 2]); 5] = [
    ("Cloudflare", ["1.1.1.1", "1.0.0.1"]),
    ("Google", ["8.8.8.8", "8.8.4.4"]),
    ("Quad9", ["9.9.9.9", "149.112.112.112"]),
    ("OpenDNS", ["208.67.222.222", "208.67.220.220"]),
    ("FDN", ["80.67.169.12", "80.67.169.40"]),
];

const RESOLV_CONF: &str = "/etc/resolv.conf";
const RESOLV_BACKUP: &str = "/etc/resolv.conf.orbisys.bak";
const RESOLVED_DROPIN_DIR: &str = "/etc/systemd/resolved.conf.d";
const RESOLVED_DROPIN: &str = "/etc/systemd/resolved.conf.d/99-orbisys.conf";

/// Une adresse IPv6 littérale plausible.
fn valid_ipv6(text: &str) -> bool {
    text.contains(':')
        && text.len() <= 45
        && text
            .chars()
            .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '.'))
}

/// Un serveur DNS acceptable (IPv4 ou IPv6).
pub fn valid_nameserver(text: &str) -> bool {
    let value = text.trim();
    !value.is_empty()
        && !value.starts_with('-')
        && (crate::panel::valid_ipv4(value) || valid_ipv6(value))
}

/// Un nom d'hôte acceptable pour `/etc/hosts`.
pub fn valid_hostname(text: &str) -> bool {
    let value = text.trim();
    !value.is_empty()
        && value.len() <= 253
        && !value.starts_with('-')
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
}

/// Analyse `/etc/hosts` (commentaires ignorés).
pub fn parse_hosts(text: &str) -> Vec<HostEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut fields = line.split_whitespace();
        let Some(address) = fields.next() else {
            continue;
        };
        if crate::panel::valid_ipv4(address) || valid_ipv6(address) {
            let names: Vec<String> = fields.map(str::to_string).collect();
            if !names.is_empty() {
                entries.push(HostEntry {
                    address: address.to_string(),
                    names,
                });
            }
        }
    }
    entries
}

/// Entrées actuelles de `/etc/hosts`.
pub fn hosts() -> Vec<HostEntry> {
    std::fs::read_to_string("/etc/hosts")
        .map(|text| parse_hosts(&text))
        .unwrap_or_default()
}

/// Écrit un fichier système via `install` (Polkit), sans shell.
fn write_root_file(path: &str, content: &str) -> Result<(), String> {
    if !path.starts_with("/etc/") {
        return Err(format!("chemin refusé : {path}"));
    }
    let temporary = util::write_private_temp("um-netcfg", content)?;
    let result = exec::run_privileged(
        "install",
        &[
            "-m".to_string(),
            "644".to_string(),
            temporary.display().to_string(),
            path.to_string(),
        ],
    );
    let _ = std::fs::remove_file(&temporary);
    result.map(|_| ())
}

/// Vrai si le DNS est géré par systemd-resolved.
fn resolved_active() -> bool {
    Path::new("/run/systemd/resolve").is_dir()
        || std::fs::read_link(RESOLV_CONF)
            .map(|target| target.to_string_lossy().contains("systemd"))
            .unwrap_or(false)
}

/// Applique un résolveur DNS (systemd-resolved si actif, sinon `/etc/resolv.conf`).
pub fn apply_dns(servers: &[String]) -> Result<String, String> {
    if servers.is_empty() || servers.len() > 3 {
        return Err("un à trois serveurs DNS attendus".to_string());
    }
    for server in servers {
        if !valid_nameserver(server) {
            return Err(format!("serveur DNS refusé : {server}"));
        }
    }
    let joined = servers.join(" ");
    if resolved_active() {
        exec::run_privileged(
            "install",
            &[
                "-d".to_string(),
                "-m".to_string(),
                "755".to_string(),
                RESOLVED_DROPIN_DIR.to_string(),
            ],
        )
        .map_err(|error| format!("dossier resolved.conf.d : {error}"))?;
        write_root_file(
            RESOLVED_DROPIN,
            &format!("# Généré par OrbiSys\n[Resolve]\nDNS={joined}\n"),
        )?;
        exec::run_privileged("systemctl", &["restart".to_string(), "systemd-resolved".to_string()])?;
        let _ = exec::run_privileged("resolvectl", &["flush-caches".to_string()]);
        return Ok(format!("résolveur systemd-resolved configuré : {joined}"));
    }
    if !Path::new(RESOLV_BACKUP).exists() {
        let current = std::fs::read_to_string(RESOLV_CONF).unwrap_or_default();
        write_root_file(RESOLV_BACKUP, &current)?;
    }
    let mut content = String::from("# Généré par OrbiSys\n");
    for server in servers {
        content.push_str(&format!("nameserver {server}\n"));
    }
    write_root_file(RESOLV_CONF, &content)?;
    Ok(format!(
        "/etc/resolv.conf réécrit : {joined} (sauvegarde {RESOLV_BACKUP})"
    ))
}

/// Ajoute une association adresse → nom dans `/etc/hosts`.
pub fn add_host(address: &str, name: &str) -> Result<String, String> {
    let address = address.trim();
    let name = name.trim();
    if !(crate::panel::valid_ipv4(address) || valid_ipv6(address)) {
        return Err(format!("adresse refusée : {address}"));
    }
    if !valid_hostname(name) {
        return Err(format!("nom d'hôte refusé : {name}"));
    }
    let current = std::fs::read_to_string("/etc/hosts").unwrap_or_default();
    if parse_hosts(&current).iter().any(|entry| {
        entry.address == address
            && entry
                .names
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(name))
    }) {
        return Ok(format!("{name} est déjà associé à {address}"));
    }
    let mut content = current.clone();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("{address}\t{name}\n"));
    write_root_file("/etc/hosts", &content)?;
    Ok(format!("{name} → {address} ajouté à /etc/hosts"))
}

/// Retire une association adresse + nom de `/etc/hosts`.
pub fn remove_host(address: &str, name: &str) -> Result<String, String> {
    let address = address.trim();
    let name = name.trim();
    if !(crate::panel::valid_ipv4(address) || valid_ipv6(address)) || !valid_hostname(name) {
        return Err("entrée invalide".to_string());
    }
    let current = std::fs::read_to_string("/etc/hosts").unwrap_or_default();
    let mut removed = false;
    let mut kept: Vec<&str> = Vec::new();
    for line in current.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let is_target = fields.len() >= 2
            && fields[0] == address
            && fields
                .iter()
                .skip(1)
                .any(|field| field.eq_ignore_ascii_case(name));
        if is_target {
            removed = true;
            continue;
        }
        kept.push(line);
    }
    if !removed {
        return Err(format!("entrée absente : {address} {name}"));
    }
    let mut content = kept.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    write_root_file("/etc/hosts", &content)?;
    Ok(format!("{name} retiré de /etc/hosts"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_links_and_addresses() {
        let links = "2: enp3s0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel state UP mode DEFAULT group default qlen 1000\\    link/ether 00:11:22:33:44:55 brd ff:ff:ff:ff:ff:ff\n1: lo: <LOOPBACK,UP> mtu 65536 state UNKNOWN\n";
        let mut interfaces = parse_links(links);
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].name, "enp3s0");
        assert_eq!(interfaces[0].state, "UP");
        assert_eq!(interfaces[0].mac, "00:11:22:33:44:55");
        parse_addrs(
            "2: enp3s0    inet 192.168.1.20/24 brd 192.168.1.255 scope global dynamic enp3s0\n",
            &mut interfaces,
        );
        assert_eq!(interfaces[0].addresses, vec!["192.168.1.20/24"]);
    }

    #[test]
    fn parses_default_route() {
        let (gateway, device) = parse_default_route(
            "default via 192.168.1.1 dev enp3s0 proto dhcp metric 100\n",
        );
        assert_eq!(gateway, "192.168.1.1");
        assert_eq!(device, "enp3s0");
        assert_eq!(parse_default_route(""), (String::new(), String::new()));
    }

    #[test]
    fn parses_resolv_conf() {
        let config = parse_resolv(
            "# commentaire\nnameserver 127.0.0.53\nnameserver 9.9.9.9\nsearch lan local\n",
        );
        assert_eq!(config.nameservers, vec!["127.0.0.53", "9.9.9.9"]);
        assert_eq!(config.search, vec!["lan", "local"]);
    }

    #[test]
    fn parses_samba_shares() {
        let text = "[global]\n   workgroup = WORKGROUP\n\n[public]\n   path = /srv/public\n   read only = no\n\n[prive]\n   path = /srv/prive\n   browseable = no\n   writable = yes\n";
        let shares = parse_smb_conf(text);
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].name, "public");
        assert!(!shares[0].read_only);
        assert!(!shares[1].browseable);
        assert!(!shares[1].read_only);
    }

    #[test]
    fn parses_vpn_connections() {
        let text = "Wired connection 1:802-3-ethernet:enp3s0\nBoulot:vpn:--\nwg0:wireguard:wg0\n";
        assert_eq!(parse_nmcli_vpn(text), vec!["Boulot", "wg0"]);
        assert_eq!(parse_wireguard("wg0 wg1\n"), vec!["wg0", "wg1"]);
    }

    #[test]
    fn parses_hosts_entries() {
        let entries = parse_hosts(
            "# commentaire\n127.0.0.1 localhost\n192.168.1.10 nas nas.local # serveur\n",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].address, "192.168.1.10");
        assert_eq!(entries[1].names, vec!["nas", "nas.local"]);
    }

    #[test]
    fn validates_nameservers_and_hostnames() {
        assert!(valid_nameserver("1.1.1.1"));
        assert!(valid_nameserver("2606:4700:4700::1111"));
        assert!(!valid_nameserver("1.1.1.1; reboot"));
        assert!(!valid_nameserver("-x"));
        assert!(!valid_nameserver(""));
        assert!(valid_hostname("nas.local"));
        assert!(valid_hostname("mon-nas"));
        assert!(!valid_hostname("nas local"));
        assert!(!valid_hostname("-nas"));
        assert!(add_host("1.1.1.1; reboot", "nas").is_err());
        assert!(remove_host("192.168.1.10", "nas; reboot").is_err());
    }

    #[test]
    fn rejects_dangerous_hosts() {
        assert!(valid_host("192.168.1.1"));
        assert!(valid_host("fr.archive.ubuntu.com"));
        assert!(!valid_host("1.1.1.1; reboot"));
        assert!(!valid_host("-c"));
        assert!(!valid_host(""));
        assert!(ping("1.1.1.1; reboot").is_err());
        assert!(resolve("a b").is_err());
    }
}
