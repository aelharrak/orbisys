//! Réseau : interfaces, Wi-Fi, DNS et hôtes, serveur SSH, partages
//! Samba/NFS et connexions actives. Tout est en lecture seule.

use crate::exec;
use serde::Serialize;

/// Une interface réseau.
#[derive(Debug, Clone, Serialize)]
pub struct Interface {
    pub name: String,
    pub state: String,
    pub mac: String,
    pub mtu: u64,
    pub addresses: Vec<String>,
}

/// Un réseau Wi-Fi visible.
#[derive(Debug, Clone, Serialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u32,
    pub security: String,
    pub active: bool,
}

/// Configuration DNS et hôtes locaux.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DnsConfig {
    pub nameservers: Vec<String>,
    pub search: Vec<String>,
    pub hosts: Vec<(String, Vec<String>)>,
}

/// État du serveur SSH.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SshServer {
    pub installed: bool,
    pub active: bool,
    pub enabled: bool,
    pub port: String,
    pub root_login: String,
    pub password_auth: String,
}

/// Un partage Samba ou NFS.
#[derive(Debug, Clone, Serialize)]
pub struct Share {
    pub kind: &'static str,
    pub name: String,
    pub path: String,
    pub detail: String,
}

/// Une connexion réseau établie.
#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    pub protocol: String,
    pub state: String,
    pub local: String,
    pub peer: String,
    pub process: String,
}

/// Rapport réseau complet.
#[derive(Debug, Clone, Default)]
pub struct NetworkReport {
    pub interfaces: Vec<Interface>,
    pub wifi: Vec<WifiNetwork>,
    pub dns: DnsConfig,
    pub ssh: SshServer,
    pub shares: Vec<Share>,
    pub connections: Vec<Connection>,
    pub errors: Vec<String>,
}

/// Analyse `ip -j addr show`.
pub fn parse_ip_addr(text: &str) -> Vec<Interface> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let name = entry.get("ifname")?.as_str()?.to_string();
            let addresses = entry
                .get("addr_info")
                .and_then(|value| value.as_array())
                .map(|addresses| {
                    addresses
                        .iter()
                        .filter_map(|address| {
                            let local = address.get("local")?.as_str()?;
                            let prefix = address
                                .get("prefixlen")
                                .and_then(|value| value.as_u64())
                                .unwrap_or(0);
                            Some(format!("{local}/{prefix}"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(Interface {
                name,
                state: entry
                    .get("operstate")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
                mac: entry
                    .get("address")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
                mtu: entry.get("mtu").and_then(|value| value.as_u64()).unwrap_or(0),
                addresses,
            })
        })
        .collect()
}

/// Analyse `/etc/resolv.conf`.
pub fn parse_resolv_conf(text: &str) -> (Vec<String>, Vec<String>) {
    let mut nameservers = Vec::new();
    let mut search = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("nameserver") => {
                if let Some(value) = fields.next() {
                    nameservers.push(value.to_string());
                }
            }
            Some("search") | Some("domain") => {
                for value in fields {
                    search.push(value.to_string());
                }
            }
            _ => {}
        }
    }
    (nameservers, search)
}

/// Analyse `/etc/hosts`.
pub fn parse_hosts(text: &str) -> Vec<(String, Vec<String>)> {
    text.lines()
        .filter_map(|line| {
            let line = line.split('#').next()?.trim();
            if line.is_empty() {
                return None;
            }
            let mut fields = line.split_whitespace();
            let address = fields.next()?.to_string();
            let names: Vec<String> = fields.map(str::to_string).collect();
            (!names.is_empty()).then_some((address, names))
        })
        .collect()
}

/// Analyse `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY device wifi list`.
pub fn parse_nmcli_wifi(text: &str) -> Vec<WifiNetwork> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.splitn(4, ':').collect();
            if fields.len() < 4 {
                return None;
            }
            let ssid = fields[1].replace("\\:", ":").trim().to_string();
            if ssid.is_empty() {
                return None;
            }
            Some(WifiNetwork {
                ssid,
                signal: fields[2].trim().parse().unwrap_or(0),
                security: fields[3].replace("\\:", ":").trim().to_string(),
                active: fields[0].trim() == "*" || fields[0].trim().eq_ignore_ascii_case("yes"),
            })
        })
        .collect()
}

/// Analyse la section partagée de `smb.conf`.
pub fn parse_smb_conf(text: &str) -> Vec<Share> {
    let mut shares = Vec::new();
    let mut current: Option<String> = None;
    let mut path = String::new();
    let mut comment = String::new();
    for line in text.lines() {
        let line = line.split(['#', ';']).next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(name) = current.take() {
                if !path.is_empty() && !name.eq_ignore_ascii_case("global") {
                    shares.push(Share {
                        kind: "Samba",
                        name,
                        path: path.clone(),
                        detail: comment.clone(),
                    });
                }
            }
            current = Some(line[1..line.len() - 1].to_string());
            path.clear();
            comment.clear();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim().to_ascii_lowercase().as_str() {
                "path" => path = value.trim().to_string(),
                "comment" => comment = value.trim().to_string(),
                _ => {}
            }
        }
    }
    if let Some(name) = current.take() {
        if !path.is_empty() && !name.eq_ignore_ascii_case("global") {
            shares.push(Share {
                kind: "Samba",
                name,
                path,
                detail: comment,
            });
        }
    }
    shares
}

/// Analyse `/etc/exports` (partages NFS).
pub fn parse_exports(text: &str) -> Vec<Share> {
    text.lines()
        .filter_map(|line| {
            let line = line.split('#').next()?.trim();
            if line.is_empty() {
                return None;
            }
            let mut fields = line.split_whitespace();
            let path = fields.next()?.to_string();
            let clients: Vec<&str> = fields.collect();
            Some(Share {
                kind: "NFS",
                name: path.rsplit('/').next().unwrap_or(&path).to_string(),
                path,
                detail: clients.join(" "),
            })
        })
        .collect()
}

/// Analyse `ss -tunpH` (connexions établies).
pub fn parse_ss(text: &str) -> Vec<Connection> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 {
                return None;
            }
            let process = line
                .split("users:(")
                .nth(1)
                .map(|rest| rest.trim_end_matches(')').to_string())
                .unwrap_or_default();
            Some(Connection {
                protocol: fields[0].to_string(),
                state: fields[1].to_string(),
                local: fields[4].to_string(),
                peer: fields[5.min(fields.len() - 1)].to_string(),
                process,
            })
        })
        .collect()
}

fn systemctl_state(unit: &str, verb: &str) -> bool {
    exec::run("systemctl", &[verb, unit])
        .map(|captured| captured.text() == "active" || captured.text() == "enabled")
        .unwrap_or(false)
}

fn ssh_server() -> SshServer {
    let config = std::fs::read_to_string("/etc/ssh/sshd_config").unwrap_or_default();
    let installed = !config.is_empty();
    let port = crate::security::sshd_option(&config, "Port").unwrap_or_else(|| "22".to_string());
    SshServer {
        installed,
        active: systemctl_state("ssh", "is-active"),
        enabled: systemctl_state("ssh", "is-enabled"),
        port,
        root_login: crate::security::sshd_option(&config, "PermitRootLogin")
            .unwrap_or_else(|| "non défini".to_string()),
        password_auth: crate::security::sshd_option(&config, "PasswordAuthentication")
            .unwrap_or_else(|| "non défini".to_string()),
    }
}

/// Collecte l'état réseau complet (lecture seule).
pub fn report() -> NetworkReport {
    let mut report = NetworkReport::default();

    match exec::run_with_timeout(
        "ip",
        &["-j", "addr", "show"],
        std::time::Duration::from_secs(15),
    ) {
        Ok(captured) if captured.ok => report.interfaces = parse_ip_addr(&captured.stdout),
        Ok(captured) => report.errors.push(captured.error_text()),
        Err(error) => report.errors.push(error.0),
    }

    if exec::exists("nmcli") {
        match exec::run_with_timeout(
            "nmcli",
            &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list"],
            std::time::Duration::from_secs(20),
        ) {
            Ok(captured) if captured.ok => report.wifi = parse_nmcli_wifi(&captured.stdout),
            Ok(_) => {}
            Err(error) => report.errors.push(error.0),
        }
    }

    if let Ok(text) = std::fs::read_to_string("/etc/resolv.conf") {
        let (nameservers, search) = parse_resolv_conf(&text);
        report.dns.nameservers = nameservers;
        report.dns.search = search;
    }
    if let Ok(text) = std::fs::read_to_string("/etc/hosts") {
        report.dns.hosts = parse_hosts(&text);
    }

    report.ssh = ssh_server();

    if let Ok(text) = std::fs::read_to_string("/etc/samba/smb.conf") {
        report.shares.extend(parse_smb_conf(&text));
    }
    if let Ok(text) = std::fs::read_to_string("/etc/exports") {
        report.shares.extend(parse_exports(&text));
    }

    if exec::exists("ss") {
        match exec::run_with_timeout(
            "ss",
            &["-tunpH"],
            std::time::Duration::from_secs(20),
        ) {
            Ok(captured) => {
                let mut connections = parse_ss(&captured.stdout);
                connections.sort_by(|a, b| a.process.cmp(&b.process));
                connections.truncate(400);
                report.connections = connections;
            }
            Err(error) => report.errors.push(error.0),
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ip_json() {
        let json = r#"[{"ifname":"lo","operstate":"UNKNOWN","address":"00:00:00:00:00:00","mtu":65536,
            "addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8}]},
            {"ifname":"wlp2s0","operstate":"UP","address":"a0:b1:c2:d3:e4:f5","mtu":1500,
            "addr_info":[{"family":"inet","local":"192.168.1.20","prefixlen":24}]}]"#;
        let interfaces = parse_ip_addr(json);
        assert_eq!(interfaces.len(), 2);
        assert_eq!(interfaces[1].name, "wlp2s0");
        assert_eq!(interfaces[1].addresses, vec!["192.168.1.20/24"]);
    }

    #[test]
    fn parses_resolv_conf() {
        let text = "# commentaire\nnameserver 1.1.1.1\nnameserver 9.9.9.9\nsearch lan local\n";
        let (servers, search) = parse_resolv_conf(text);
        assert_eq!(servers, vec!["1.1.1.1", "9.9.9.9"]);
        assert_eq!(search, vec!["lan", "local"]);
    }

    #[test]
    fn parses_hosts_file() {
        let text = "127.0.0.1 localhost\n# commentaire\n192.168.1.10 nas nas.lan\n";
        let hosts = parse_hosts(text);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[1].1, vec!["nas", "nas.lan"]);
    }

    #[test]
    fn parses_nmcli_wifi() {
        let text = "*:Maison:82:WPA2\n :Voisin:41:WPA2\n";
        let networks = parse_nmcli_wifi(text);
        assert_eq!(networks.len(), 2);
        assert!(networks[0].active);
        assert_eq!(networks[0].signal, 82);
    }

    #[test]
    fn parses_smb_conf_shares() {
        let text = "[global]\n   workgroup = WORKGROUP\n\n[partage]\n   path = /srv/partage\n   comment = Documents\n";
        let shares = parse_smb_conf(text);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].name, "partage");
        assert_eq!(shares[0].path, "/srv/partage");
    }

    #[test]
    fn parses_nfs_exports() {
        let text = "/srv/nfs 192.168.1.0/24(rw,sync,no_subtree_check)\n";
        let shares = parse_exports(text);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].kind, "NFS");
        assert_eq!(shares[0].name, "nfs");
    }

    #[test]
    fn parses_ss_output() {
        let text = "tcp ESTAB 0 0 192.168.1.20:22 192.168.1.5:51234 users:((\"sshd\",pid=999,fd=3))\n";
        let connections = parse_ss(text);
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].protocol, "tcp");
        assert_eq!(connections[0].peer, "192.168.1.5:51234");
        assert!(connections[0].process.contains("sshd"));
    }

    #[test]
    fn report_is_collectable() {
        let report = report();
        assert!(report.errors.len() < 10);
    }
}
