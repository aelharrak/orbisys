//! Réseau : trafic par processus et découverte du réseau local.

use crate::exec;
use crate::p_monitor;
use crate::panel::*;
use crate::util;

// ---------------------------------------------------------------------------
// Trafic réseau
// ---------------------------------------------------------------------------

/// Une connexion ou un socket en écoute.
#[derive(Debug, Clone, PartialEq)]
pub struct Connection {
    pub proto: String,
    pub state: String,
    pub local: String,
    pub peer: String,
    pub process: String,
    pub pid: Option<u32>,
}

/// Extrait `("nom", pid)` de `users:(("firefox",pid=1234,fd=5))`.
pub fn parse_users_field(field: &str) -> (String, Option<u32>) {
    let Some(start) = field.find("((\"") else {
        return (String::new(), None);
    };
    let rest = &field[start + 3..];
    let Some(end) = rest.find('"') else {
        return (String::new(), None);
    };
    let name = rest[..end].to_string();
    let pid = rest
        .split("pid=")
        .nth(1)
        .and_then(|tail| tail.split(|ch: char| !ch.is_ascii_digit()).next())
        .and_then(|digits| digits.parse().ok());
    (name, pid)
}

/// Analyse `ss -H -tunap`.
pub fn parse_ss(text: &str) -> Vec<Connection> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 6 {
                return None;
            }
            let (process, pid) = fields
                .get(6)
                .map(|field| parse_users_field(field))
                .unwrap_or_default();
            Some(Connection {
                proto: fields[0].to_string(),
                state: fields[1].to_string(),
                local: fields[4].to_string(),
                peer: fields[5].to_string(),
                process,
                pid,
            })
        })
        .collect()
}

pub fn traffic() -> Panel {
    let mut panel = Panel::default();
    match run_text("ss", &["-H", "-tunap"]) {
        Err(error) => panel.error(format!("ss : {error}")),
        Ok(text) => {
            let connections = parse_ss(&text);
            let established = connections.iter().filter(|c| c.state == "ESTAB").count();
            let listening = connections.iter().filter(|c| c.state == "LISTEN").count();
            panel.metric("Connexions établies", established.to_string(), Level::Info);
            panel.metric("Ports en écoute", listening.to_string(), Level::Info);
            let mut per_process: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
            for connection in connections.iter().filter(|c| c.state == "ESTAB") {
                let name = if connection.process.is_empty() { "(inconnu)" } else { &connection.process };
                *per_process.entry(name.to_string()).or_default() += 1;
            }
            let mut ranked: Vec<(String, usize)> = per_process.into_iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let rows = ranked
                .iter()
                .map(|(name, count)| Row::owned(vec![name.clone(), count.to_string()]))
                .collect();
            panel.section(
                Section::new("Connexions établies par processus")
                    .columns(&["Processus", "Connexions"])
                    .rows(rows)
                    .note("Les processus d'autres utilisateurs n'apparaissent qu'avec les droits administrateur."),
            );
            let rows = connections
                .iter()
                .filter(|c| c.state == "ESTAB")
                .take(200)
                .map(|c| {
                    Row::owned(vec![
                        c.proto.clone(),
                        c.local.clone(),
                        c.peer.clone(),
                        match c.pid {
                            Some(pid) => format!("{} ({pid})", c.process),
                            None => c.process.clone(),
                        },
                    ])
                })
                .collect();
            panel.section(
                Section::new("Détail des connexions")
                    .columns(&["Protocole", "Local", "Distant", "Processus"])
                    .rows(rows),
            );
        }
    }
    let counters: Vec<(String, u64, u64)> = p_monitor::parse_net_dev(&read("/proc/net/dev"))
        .into_iter()
        .filter(|(name, _, _)| name != "lo")
        .collect();
    let chart = Chart::bars(
        "Volume échangé par interface (reçu + émis)",
        "B",
        counters.iter().map(|(name, _, _)| name.clone()).collect(),
        counters.iter().map(|(_, rx, tx)| (*rx + *tx) as f32).collect(),
    );
    let rows = p_monitor::parse_net_dev(&read("/proc/net/dev"))
        .into_iter()
        .map(|(name, rx, tx)| {
            Row::owned(vec![name, util::human_bytes(rx), util::human_bytes(tx)])
        })
        .collect();
    panel.section(
        Section::new("Compteurs par interface (depuis le démarrage)")
            .chart(chart)
            .columns(&["Interface", "Reçu", "Émis"])
            .rows(rows),
    );
    panel
}

// ---------------------------------------------------------------------------
// Réseau local
// ---------------------------------------------------------------------------

/// Un hôte découvert.
#[derive(Debug, Clone, PartialEq)]
pub struct Host {
    pub ip: String,
    pub name: String,
    pub mac: String,
    pub source: &'static str,
}

/// Analyse `/proc/net/arp` : seules les entrées résolues sont gardées.
pub fn parse_arp(text: &str) -> Vec<Host> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 6 || fields[2] == "0x0" || !valid_ipv4(fields[0]) {
                return None;
            }
            Some(Host {
                ip: fields[0].to_string(),
                name: String::new(),
                mac: fields[3].to_string(),
                source: "ARP",
            })
        })
        .collect()
}

/// Analyse `nmap -sn -oG -`.
pub fn parse_nmap_grepable(text: &str) -> Vec<Host> {
    text.lines()
        .filter(|line| line.starts_with("Host:") && line.contains("Status: Up"))
        .filter_map(|line| {
            let rest = line.strip_prefix("Host:")?.trim();
            let ip = rest.split_whitespace().next()?;
            if !valid_ipv4(ip) {
                return None;
            }
            let name = rest
                .split('(')
                .nth(1)
                .and_then(|tail| tail.split(')').next())
                .unwrap_or("")
                .to_string();
            Some(Host {
                ip: ip.to_string(),
                name,
                mac: String::new(),
                source: "nmap",
            })
        })
        .collect()
}

/// Réseau IPv4 de la première interface globale, ramené à /24 au plus large :
/// `("192.168.1.0/24")`. Refuse tout ce qui n'est pas un réseau privé.
pub fn local_cidr(addr_output: &str) -> Option<String> {
    for line in addr_output.lines() {
        let token = line.split_whitespace().find(|token| token.contains('/') && token.contains('.'))?;
        let (ip, prefix) = token.split_once('/')?;
        let prefix: u8 = prefix.parse().ok()?;
        if !valid_ipv4(ip) {
            continue;
        }
        let octets: Vec<u8> = ip.split('.').filter_map(|part| part.parse().ok()).collect();
        let private = octets[0] == 10
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            || (octets[0] == 192 && octets[1] == 168);
        if !private {
            continue;
        }
        let prefix = prefix.clamp(24, 30);
        let mask: u32 = u32::MAX << (32 - prefix);
        let value = u32::from_be_bytes([octets[0], octets[1], octets[2], octets[3]]) & mask;
        let net = value.to_be_bytes();
        return Some(format!("{}.{}.{}.{}/{prefix}", net[0], net[1], net[2], net[3]));
    }
    None
}

/// Fusionne les listes en gardant une entrée par adresse IP.
pub fn merge_hosts(lists: Vec<Vec<Host>>) -> Vec<Host> {
    let mut merged: Vec<Host> = Vec::new();
    for host in lists.into_iter().flatten() {
        match merged.iter_mut().find(|known| known.ip == host.ip) {
            Some(known) => {
                if known.name.is_empty() {
                    known.name = host.name;
                }
                if known.mac.is_empty() {
                    known.mac = host.mac;
                }
            }
            None => merged.push(host),
        }
    }
    merged.sort_by(|a, b| {
        let key = |ip: &str| -> Vec<u8> { ip.split('.').filter_map(|p| p.parse().ok()).collect() };
        key(&a.ip).cmp(&key(&b.ip))
    });
    merged
}

pub fn lan_discovery() -> Panel {
    let mut panel = Panel::default();
    let mut lists = vec![parse_arp(&read("/proc/net/arp"))];
    let cidr = local_cidr(&run_lossy("ip", &["-4", "-o", "addr", "show", "scope", "global"]));
    match (&cidr, exec::exists("nmap")) {
        (Some(cidr), true) => match run_text("nmap", &["-sn", "-n", "-oG", "-", cidr]) {
            Ok(text) => lists.push(parse_nmap_grepable(&text)),
            Err(error) => panel.error(format!("nmap : {error}")),
        },
        (None, _) => panel.error("Aucun réseau privé IPv4 détecté : seule la table ARP est affichée."),
        (Some(_), false) => panel.error("nmap absent : seule la table ARP du noyau est affichée."),
    }
    let hosts = merge_hosts(lists);
    panel.metric("Hôtes", hosts.len().to_string(), Level::Info);
    if let Some(cidr) = &cidr {
        panel.metric("Réseau", cidr.clone(), Level::Info);
    }
    let rows = hosts
        .iter()
        .map(|host| Row::new(&[&host.ip, &host.name, &host.mac, host.source]))
        .collect();
    panel.section(
        Section::new("Hôtes du réseau local")
            .columns(&["Adresse IP", "Nom", "Adresse MAC", "Source"])
            .rows(rows)
            .note("Le scan est limité au réseau privé local (préfixe /24 ou plus étroit) ; actualisez l'écran pour le relancer."),
    );
    panel
}

// ---------------------------------------------------------------------------
// Wi-Fi et VPN (NetworkManager)
// ---------------------------------------------------------------------------

/// Découpe une ligne `nmcli -t` en champs (`\:` et `\\` sont des échappements).
pub fn split_nmcli_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    fields.last_mut().unwrap().push(next);
                }
            }
            ':' => fields.push(String::new()),
            other => fields.last_mut().unwrap().push(other),
        }
    }
    fields
}

/// Une connexion NetworkManager.
#[derive(Debug, Clone, PartialEq)]
pub struct NmConnection {
    pub name: String,
    pub uuid: String,
    pub kind: String,
    pub device: String,
}

impl NmConnection {
    pub fn is_vpn(&self) -> bool {
        self.kind == "vpn" || self.kind == "wireguard"
    }
}

/// Analyse `nmcli -t -f NAME,UUID,TYPE,DEVICE connection show`.
pub fn parse_nm_connections(text: &str) -> Vec<NmConnection> {
    text.lines()
        .filter_map(|line| {
            let fields = split_nmcli_terse(line);
            if fields.len() < 4 {
                return None;
            }
            Some(NmConnection {
                name: fields[0].clone(),
                uuid: fields[1].clone(),
                kind: fields[2].clone(),
                device: fields[3].clone(),
            })
        })
        .collect()
}

/// UUID `8-4-4-4-12` hexadécimal.
pub fn valid_uuid(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let sizes = [8, 4, 4, 4, 12];
    parts.len() == 5
        && parts
            .iter()
            .zip(sizes)
            .all(|(part, size)| part.len() == size && part.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn build_vpn_import(values: &[String]) -> Result<Cmd, String> {
    let [kind, file] = values else {
        return Err("type et fichier attendus".to_string());
    };
    let (kind, file) = (kind.trim(), file.trim());
    let extension = match kind {
        "wireguard" => ".conf",
        "openvpn" => ".ovpn",
        _ => return Err("type inconnu : wireguard ou openvpn".to_string()),
    };
    if !safe_abs_path(file) || !file.to_ascii_lowercase().ends_with(extension) {
        return Err(format!("fichier {extension} avec chemin absolu requis"));
    }
    if !std::path::Path::new(file).is_file() {
        return Err(format!("fichier introuvable : {file}"));
    }
    Ok(Cmd::new("Import VPN", "nmcli", &["connection", "import", "type", kind, "file", file]))
}

pub fn wifi_vpn() -> Panel {
    let mut panel = Panel::default();
    if !exec::exists("nmcli") {
        panel.error("nmcli absent : NetworkManager est requis pour gérer le Wi-Fi et les VPN.");
        return panel;
    }
    let radio = run_lossy("nmcli", &["radio", "wifi"]).trim().to_string();
    let on = radio == "enabled";
    panel.metric("Radio Wi-Fi", if radio.is_empty() { "inconnue".to_string() } else { radio.clone() }, if on { Level::Ok } else { Level::Warn });
    panel.section(
        Section::new("Radio Wi-Fi")
            .action(if on {
                Cmd::new("Désactiver le Wi-Fi", "nmcli", &["radio", "wifi", "off"]).confirm("Couper la radio Wi-Fi ? La connexion sans fil sera perdue.")
            } else {
                Cmd::new("Activer le Wi-Fi", "nmcli", &["radio", "wifi", "on"])
            }),
    );
    if on {
        let networks = crate::network::parse_nmcli_wifi(&run_lossy(
            "nmcli",
            &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list"],
        ));
        let rows = networks
            .iter()
            .map(|network| {
                Row::owned(vec![
                    network.ssid.clone(),
                    format!("{} %", network.signal),
                    if network.security.is_empty() { "ouvert".into() } else { network.security.clone() },
                    if network.active { "connecté".into() } else { String::new() },
                ])
                .bad(network.security.is_empty())
            })
            .collect();
        panel.section(
            Section::new("Réseaux détectés")
                .columns(&["SSID", "Signal", "Sécurité", "État"])
                .rows(rows),
        );
    }
    let connections = parse_nm_connections(&run_lossy("nmcli", &["-t", "-f", "NAME,UUID,TYPE,DEVICE", "connection", "show"]));
    let vpns: Vec<&NmConnection> = connections.iter().filter(|c| c.is_vpn() && valid_uuid(&c.uuid)).collect();
    panel.metric("VPN configurés", vpns.len().to_string(), Level::Info);
    let rows = vpns
        .iter()
        .map(|vpn| {
            let up = !vpn.device.is_empty();
            Row::owned(vec![vpn.name.clone(), vpn.kind.clone(), if up { "actif" } else { "inactif" }.to_string()])
                .action(if up {
                    Cmd::new("Déconnecter", "nmcli", &["connection", "down", "uuid", &vpn.uuid])
                } else {
                    Cmd::new("Connecter", "nmcli", &["connection", "up", "uuid", &vpn.uuid])
                })
                .action(
                    Cmd::new("Supprimer", "nmcli", &["connection", "delete", "uuid", &vpn.uuid])
                        .confirm(&format!("Supprimer la connexion VPN « {} » ?", vpn.name)),
                )
        })
        .collect();
    panel.section(
        Section::new("Connexions VPN")
            .columns(&["Nom", "Type", "État"])
            .rows(rows)
            .form(Form {
                fields: vec![
                    Field::new("Type", "wireguard ou openvpn").with_default("wireguard"),
                    Field::new("Fichier", "/chemin/config.conf"),
                ],
                submit: "Importer".to_string(),
                build: build_vpn_import,
            }),
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ss_output() {
        let text = "tcp ESTAB 0 0 192.168.1.5:44312 142.250.1.1:443 users:((\"firefox\",pid=2345,fd=77))\ntcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:*\nudp UNCONN 0 0 127.0.0.53%lo:53 0.0.0.0:*\n";
        let parsed = parse_ss(text);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].process, "firefox");
        assert_eq!(parsed[0].pid, Some(2345));
        assert_eq!(parsed[1].process, "");
        assert_eq!(parsed[1].state, "LISTEN");
    }

    #[test]
    fn parses_users_field_edge_cases() {
        assert_eq!(parse_users_field("users:((\"a\",pid=1,fd=2),(\"b\",pid=3,fd=4))"), ("a".to_string(), Some(1)));
        assert_eq!(parse_users_field("rien"), (String::new(), None));
    }

    #[test]
    fn parses_arp_table() {
        let text = "IP address HW type Flags HW address Mask Device\n192.168.1.1 0x1 0x2 aa:bb:cc:dd:ee:ff * eth0\n192.168.1.9 0x1 0x0 00:00:00:00:00:00 * eth0\n";
        let hosts = parse_arp(text);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].mac, "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn parses_nmap_grepable() {
        let text = "# Nmap\nHost: 192.168.1.1 (routeur.lan)\tStatus: Up\nHost: 192.168.1.20 ()\tStatus: Up\nHost: 192.168.1.30 ()\tStatus: Down\n";
        let hosts = parse_nmap_grepable(text);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].name, "routeur.lan");
        assert_eq!(hosts[1].name, "");
    }

    #[test]
    fn computes_private_cidr_only() {
        let wifi = "2: wlp2s0    inet 192.168.1.42/24 brd 192.168.1.255 scope global dynamic wlp2s0";
        assert_eq!(local_cidr(wifi), Some("192.168.1.0/24".to_string()));
        let wide = "3: eth0    inet 10.4.7.9/8 brd 10.255.255.255 scope global eth0";
        assert_eq!(local_cidr(wide), Some("10.4.7.0/24".to_string()));
        let public = "2: eth0    inet 51.38.1.9/24 brd 51.38.1.255 scope global eth0";
        assert_eq!(local_cidr(public), None);
        assert_eq!(local_cidr(""), None);
    }

    #[test]
    fn merges_hosts_by_ip_and_sorts() {
        let arp = vec![Host { ip: "192.168.1.10".into(), name: String::new(), mac: "aa".into(), source: "ARP" }];
        let scan = vec![
            Host { ip: "192.168.1.9".into(), name: "x".into(), mac: String::new(), source: "nmap" },
            Host { ip: "192.168.1.10".into(), name: "nas".into(), mac: String::new(), source: "nmap" },
        ];
        let merged = merge_hosts(vec![arp, scan]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].ip, "192.168.1.9");
        assert_eq!(merged[1].name, "nas");
        assert_eq!(merged[1].mac, "aa");
    }

    #[test]
    fn splits_terse_nmcli_lines() {
        assert_eq!(split_nmcli_terse("Wi\\:Fi maison:abc:wifi:wlp2s0"), vec!["Wi:Fi maison", "abc", "wifi", "wlp2s0"]);
        assert_eq!(split_nmcli_terse("a\\\\b:c"), vec!["a\\b", "c"]);
        assert_eq!(split_nmcli_terse(""), vec![""]);
    }

    #[test]
    fn parses_connections_and_detects_vpn() {
        let text = "Maison:0b9d3a3c-7b2e-4d0a-9d4c-1f2e3a4b5c6d:802-11-wireless:wlp2s0\nTravail VPN:11111111-2222-3333-4444-555555555555:wireguard:\nOVH:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee:vpn:\n";
        let connections = parse_nm_connections(text);
        assert_eq!(connections.len(), 3);
        assert!(!connections[0].is_vpn());
        assert!(connections[1].is_vpn() && connections[2].is_vpn());
        assert!(connections[1].device.is_empty());
    }

    #[test]
    fn validates_uuid() {
        assert!(valid_uuid("0b9d3a3c-7b2e-4d0a-9d4c-1f2e3a4b5c6d"));
        assert!(!valid_uuid("0b9d3a3c-7b2e-4d0a-9d4c"));
        assert!(!valid_uuid("--help"));
        assert!(!valid_uuid("zzzzzzzz-7b2e-4d0a-9d4c-1f2e3a4b5c6d"));
    }

    #[test]
    fn vpn_import_is_validated() {
        let dir = std::env::temp_dir();
        let conf = dir.join(format!("um-vpn-{}.conf", std::process::id()));
        std::fs::write(&conf, "[Interface]").unwrap();
        let path = conf.to_string_lossy().to_string();
        let cmd = build_vpn_import(&["wireguard".into(), path.clone()]).unwrap();
        assert_eq!(cmd.args[..4], ["connection", "import", "type", "wireguard"]);
        assert!(build_vpn_import(&["openvpn".into(), path.clone()]).is_err(), "mauvaise extension");
        assert!(build_vpn_import(&["ipsec".into(), path]).is_err());
        assert!(build_vpn_import(&["wireguard".into(), "relatif.conf".into()]).is_err());
        assert!(build_vpn_import(&["wireguard".into(), "/inexistant/x.conf".into()]).is_err());
        let _ = std::fs::remove_file(conf);
    }
}
