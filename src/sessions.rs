//! Sessions et utilisateurs : sessions actives, dernières connexions,
//! état de verrouillage des comptes et fermeture de session.

use crate::exec;
use serde::Serialize;

/// Nom d'utilisateur Linux acceptable.
pub fn valid_username(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    name.len() <= 32 && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Une session active.
#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub user: String,
    pub tty: String,
    pub seat: String,
    pub host: String,
}

/// Une connexion passée ou en cours.
#[derive(Debug, Clone, Serialize)]
pub struct LastLogin {
    pub user: String,
    pub tty: String,
    pub host: String,
    pub when: String,
    pub active: bool,
}

/// État de verrouillage d'un compte.
#[derive(Debug, Clone, Serialize)]
pub struct LockState {
    pub user: String,
    pub locked: bool,
    pub detail: String,
}

/// Rapport des sessions.
#[derive(Debug, Clone, Default)]
pub struct SessionsReport {
    pub sessions: Vec<Session>,
    pub logins: Vec<LastLogin>,
    pub locks: Vec<LockState>,
    pub errors: Vec<String>,
}

impl SessionsReport {
    pub fn users(&self) -> Vec<String> {
        let mut users: Vec<String> = self
            .sessions
            .iter()
            .map(|session| session.user.clone())
            .chain(self.logins.iter().map(|login| login.user.clone()))
            .collect();
        users.sort();
        users.dedup();
        users
    }
}

/// Identifiant de session `loginctl` (numérique ou `c1`, `c2`…).
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Analyse `who -u`.
pub fn parse_who(text: &str) -> Vec<Session> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 6 || !valid_username(fields[0]) {
                return None;
            }
            let pid_like = fields
                .iter()
                .find(|field| field.chars().all(|c| c.is_ascii_digit()) && field.len() <= 7);
            Some(Session {
                id: pid_like.unwrap_or(&"0").to_string(),
                user: fields[0].to_string(),
                tty: fields[1].to_string(),
                seat: String::new(),
                host: fields.last().unwrap_or(&"").trim_matches(['(', ')']).to_string(),
            })
        })
        .collect()
}

/// Analyse `loginctl list-sessions --no-legend`.
pub fn parse_loginctl(text: &str) -> Vec<Session> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 3 || !valid_session_id(fields[0]) {
                return None;
            }
            let user = fields[2].to_string();
            if !valid_username(&user) {
                return None;
            }
            Some(Session {
                id: fields[0].to_string(),
                user,
                seat: fields.get(3).unwrap_or(&"").to_string(),
                tty: fields.get(4).unwrap_or(&"").to_string(),
                host: String::new(),
            })
        })
        .collect()
}

/// Analyse `last -n 25`.
pub fn parse_last(text: &str) -> Vec<LastLogin> {
    let mut logins: Vec<LastLogin> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("wtmp")
            || trimmed.starts_with("btmp")
            || trimmed.starts_with("reboot")
        {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.len() < 4 || !valid_username(fields[0]) {
            continue;
        }
        let active = trimmed.contains("still logged in") || trimmed.contains("still running");
        // La date commence après l'hôte ; on garde la fin de ligne lisible.
        let when = fields[3..].join(" ");
        logins.push(LastLogin {
            user: fields[0].to_string(),
            tty: fields[1].to_string(),
            host: fields[2].to_string(),
            when,
            active,
        });
    }
    logins.truncate(25);
    logins
}

/// Analyse `passwd -S utilisateur` (`nom L date …`).
pub fn parse_passwd_status(text: &str) -> Option<LockState> {
    let line = text.lines().next()?.trim();
    let mut fields = line.split_whitespace();
    let user = fields.next()?.to_string();
    let status = fields.next()?;
    Some(LockState {
        user,
        locked: status == "L" || status == "LK",
        detail: if status == "L" || status == "LK" {
            "compte verrouillé".to_string()
        } else if status == "P" {
            "mot de passe actif".to_string()
        } else if status == "NP" {
            "aucun mot de passe".to_string()
        } else {
            format!("état {status}")
        },
    })
}

/// Liste les sessions actives (loginctl, repli sur `who`).
pub fn sessions() -> Result<Vec<Session>, String> {
    if let Ok(captured) = exec::run_with_timeout(
        "loginctl",
        &["list-sessions", "--no-legend"],
        std::time::Duration::from_secs(15),
    ) {
        if captured.ok {
            let sessions = parse_loginctl(&captured.stdout);
            if !sessions.is_empty() {
                return Ok(sessions);
            }
        }
    }
    exec::run_with_timeout("who", &["-u"], std::time::Duration::from_secs(15))
        .map(|captured| parse_who(&captured.stdout))
        .map_err(|error| error.0)
}

/// État de verrouillage d'un utilisateur.
pub fn lock_state(user: &str) -> Option<LockState> {
    if !valid_username(user) {
        return None;
    }
    exec::run("passwd", &["-S", user])
        .ok()
        .and_then(|captured| parse_passwd_status(&captured.stdout))
}

/// Verrouille ou déverrouille un compte.
pub fn set_locked(user: &str, locked: bool) -> Result<String, String> {
    if !valid_username(user) || user == "root" {
        return Err(format!("utilisateur refusé : {user}"));
    }
    exec::run_privileged(
        "usermod",
        &[
            if locked { "-L" } else { "-U" }.to_string(),
            user.to_string(),
        ],
    )?;
    Ok(format!(
        "{} : {user}",
        if locked {
            "compte verrouillé"
        } else {
            "compte déverrouillé"
        }
    ))
}

/// Ferme une session ouverte.
pub fn terminate(id: &str) -> Result<String, String> {
    if !valid_session_id(id) {
        return Err(format!("session refusée : {id}"));
    }
    exec::run_privileged("loginctl", &["terminate-session".to_string(), id.to_string()])?;
    Ok(format!("session {id} fermée"))
}

/// Collecte le rapport des sessions.
pub fn report() -> SessionsReport {
    let mut report = SessionsReport::default();
    match sessions() {
        Ok(sessions) => report.sessions = sessions,
        Err(error) => report.errors.push(format!("sessions : {error}")),
    }
    match exec::run_with_timeout("last", &["-n", "25"], std::time::Duration::from_secs(20)) {
        Ok(captured) => report.logins = parse_last(&captured.stdout),
        Err(error) => report.errors.push(format!("last : {}", error.0)),
    }
    for user in report.users().into_iter().take(12) {
        if let Some(state) = lock_state(&user) {
            report.locks.push(state);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loginctl_sessions() {
        let text = "  1 1000 amine seat0 tty2\nc1 118 gdm seat0 tty1\n";
        let sessions = parse_loginctl(text);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "1");
        assert_eq!(sessions[0].user, "amine");
        assert_eq!(sessions[0].tty, "tty2");
        assert_eq!(sessions[1].id, "c1");
    }

    #[test]
    fn parses_who_output() {
        let text = "amine    tty7         2026-11-02 09:00   old         1234 (:0)\n";
        let sessions = parse_who(text);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].user, "amine");
        assert_eq!(sessions[0].tty, "tty7");
        assert_eq!(sessions[0].id, "1234");
    }

    #[test]
    fn parses_last_logins() {
        let text = "amine    pts/0    192.168.1.5      Mon Nov  2 09:00   still logged in\namine    tty7     :0               Sun Nov  1 08:00 - 18:00  (10:00)\n\nwtmp begins Mon Sep  1 00:00:00 2026\nreboot   system boot  6.8.0-45-generic Mon Nov  2 08:59   still running\n";
        let logins = parse_last(text);
        assert_eq!(logins.len(), 2);
        assert!(logins[0].active);
        assert_eq!(logins[0].host, "192.168.1.5");
        assert!(!logins[1].active);
    }

    #[test]
    fn parses_passwd_lock_status() {
        let locked = parse_passwd_status("amine L 2026-01-01 0 99999 7 -1\n").unwrap();
        assert!(locked.locked);
        let active = parse_passwd_status("amine P 2026-01-01 0 99999 7 -1\n").unwrap();
        assert!(!active.locked);
        assert_eq!(active.detail, "mot de passe actif");
    }

    #[test]
    fn rejects_dangerous_values() {
        assert!(valid_session_id("12"));
        assert!(valid_session_id("c1"));
        assert!(!valid_session_id("1; reboot"));
        assert!(!valid_session_id(""));
        assert!(terminate("1;reboot").is_err());
        assert!(set_locked("root", true).is_err());
        assert!(set_locked("a b", true).is_err());
    }
}
