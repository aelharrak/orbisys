//! Exécution de commandes système : jamais de shell, toujours une liste
//! d'arguments, avec délai maximal et capture séparée de stdout/stderr.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Erreur d'exécution (binaire absent, délai dépassé, échec).
#[derive(Debug, Clone)]
pub struct ExecError(pub String);

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExecError {}

/// Résultat d'une commande terminée.
#[derive(Debug, Clone, Default)]
pub struct Captured {
    pub code: Option<i32>,
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Captured {
    /// Stdout nettoyé.
    pub fn text(&self) -> String {
        self.stdout.trim().to_string()
    }

    /// Message d'erreur lisible : stderr s'il existe, sinon le code de sortie.
    pub fn error_text(&self) -> String {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return stderr.lines().take(6).collect::<Vec<_>>().join("\n");
        }
        match self.code {
            Some(code) => format!("échec (code {code})"),
            None => "échec (processus interrompu)".to_string(),
        }
    }
}

/// Chemin absolu d'un binaire s'il est dans `PATH`.
pub fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return path.is_file().then_some(path);
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Vrai si le programme est disponible.
pub fn exists(program: &str) -> bool {
    which(program).is_some()
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Vrai si le processus courant tourne en root.
#[cfg(unix)]
pub fn is_root() -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self")
        .map(|meta| meta.uid() == 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn is_root() -> bool {
    false
}

/// Lance une commande et attend sa fin (délai par défaut : 60 s).
pub fn run(program: &str, args: &[&str]) -> Result<Captured, ExecError> {
    run_with_timeout(program, args, Duration::from_secs(60))
}

/// Lance une commande avec un délai explicite.
pub fn run_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Captured, ExecError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ExecError(format!("impossible de lancer {program} : {error}")))?;

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let stdout_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(error) => return Err(ExecError(error.to_string())),
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_reader.join().unwrap_or_default()).to_string();
    let stderr = String::from_utf8_lossy(&stderr_reader.join().unwrap_or_default()).to_string();

    match status {
        Some(status) => Ok(Captured {
            code: status.code(),
            ok: status.success(),
            stdout,
            stderr,
        }),
        None => Err(ExecError(format!(
            "délai dépassé ({} s) pour {program}",
            timeout.as_secs()
        ))),
    }
}

/// Prépare une commande privilégiée : directe en root, sinon via `pkexec`.
pub fn privileged(program: &str, args: &[String]) -> (String, Vec<String>) {
    if is_root() {
        return (program.to_string(), args.to_vec());
    }
    let mut wrapped = Vec::with_capacity(args.len() + 1);
    wrapped.push(program.to_string());
    wrapped.extend_from_slice(args);
    ("pkexec".to_string(), wrapped)
}

/// Exécute une commande privilégiée et renvoie un message lisible.
pub fn run_privileged(program: &str, args: &[String]) -> Result<String, String> {
    let (real_program, real_args) = privileged(program, args);
    let refs: Vec<&str> = real_args.iter().map(String::as_str).collect();
    let captured = run_with_timeout(&real_program, &refs, Duration::from_secs(600))
        .map_err(|error| error.0)?;
    if captured.ok {
        Ok(captured.text())
    } else {
        Err(captured.error_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_without_shell() {
        let captured = run("echo", &["bonjour; rm -rf /"]).expect("echo doit exister");
        assert!(captured.ok);
        // Le point-virgule est une donnée, pas un séparateur de commandes.
        assert_eq!(captured.text(), "bonjour; rm -rf /");
    }

    #[test]
    fn reports_missing_binary() {
        let error = run("binaire-qui-nexiste-pas-12345", &[]).unwrap_err();
        assert!(error.0.contains("impossible de lancer"));
    }

    #[test]
    fn timeout_kills_command() {
        let started = Instant::now();
        let error = run_with_timeout("sleep", &["5"], Duration::from_millis(120)).unwrap_err();
        assert!(error.0.contains("délai dépassé"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
