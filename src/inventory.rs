//! Inventaire unifié : paquets natifs + Snap + Flatpak + logiciels manuels.

use crate::distro::Capabilities;
use crate::manual::{self, ManualApp};
use crate::pkg::{self, Package, PackageManager, Source};
use std::collections::HashSet;

/// Résultat d'un inventaire complet.
#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub manager: Option<PackageManager>,
    pub packages: Vec<Package>,
    pub manual: Vec<ManualApp>,
    pub errors: Vec<String>,
}

impl Inventory {
    /// Charge l'inventaire complet. Les erreurs d'une source n'empêchent pas
    /// les autres de remonter : elles sont collectées séparément.
    pub fn load(capabilities: &Capabilities) -> Self {
        let mut inventory = Inventory {
            manager: PackageManager::detect(),
            ..Default::default()
        };

        if let Some(manager) = inventory.manager {
            match manager.installed() {
                Ok(mut packages) => inventory.packages.append(&mut packages),
                Err(error) => inventory
                    .errors
                    .push(format!("{} : {error}", manager.label())),
            }
        } else {
            inventory
                .errors
                .push("aucun gestionnaire de paquets reconnu".to_string());
        }

        if capabilities.snap {
            match pkg::snap_installed() {
                Ok(mut packages) => inventory.packages.append(&mut packages),
                Err(error) => inventory.errors.push(format!("Snap : {error}")),
            }
        }
        if capabilities.flatpak {
            match pkg::flatpak_installed() {
                Ok(mut packages) => inventory.packages.append(&mut packages),
                Err(error) => inventory.errors.push(format!("Flatpak : {error}")),
            }
        }

        let known: HashSet<String> = inventory
            .packages
            .iter()
            .map(|package| package.name.to_lowercase())
            .collect();
        inventory.manual = manual::detect(&known);

        inventory
            .packages
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        inventory
    }

    /// Nombre de logiciels par source : (natif, snap, flatpak, manuel).
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut native = 0;
        let mut snap = 0;
        let mut flatpak = 0;
        for package in &self.packages {
            match package.source {
                Source::Native => native += 1,
                Source::Snap => snap += 1,
                Source::Flatpak => flatpak += 1,
                Source::Manual => {}
            }
        }
        (native, snap, flatpak, self.manual.len())
    }

    /// Taille cumulée des paquets dont la taille est connue.
    pub fn known_size(&self) -> u64 {
        self.packages.iter().map(|package| package.size_bytes).sum()
    }

    /// Filtre texte (nom, description, section) et source facultative.
    pub fn filtered(&self, query: &str, source: Option<Source>) -> Vec<&Package> {
        let needle = query.trim().to_lowercase();
        self.packages
            .iter()
            .filter(|package| source.map(|s| s == package.source).unwrap_or(true))
            .filter(|package| {
                needle.is_empty()
                    || package.name.to_lowercase().contains(&needle)
                    || package.description.to_lowercase().contains(&needle)
                    || package.section.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Logiciels manuels filtrés par recherche.
    pub fn filtered_manual(&self, query: &str) -> Vec<&ManualApp> {
        let needle = query.trim().to_lowercase();
        self.manual
            .iter()
            .filter(|app| needle.is_empty() || app.name.to_lowercase().contains(&needle))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, source: Source) -> Package {
        Package::new(name, "1.0", source)
    }

    #[test]
    fn counts_and_filters() {
        let inventory = Inventory {
            manager: Some(PackageManager::Apt),
            packages: vec![
                package("firefox", Source::Native),
                package("code", Source::Snap),
                package("org.gnome.Builder", Source::Flatpak),
            ],
            manual: Vec::new(),
            errors: Vec::new(),
        };
        assert_eq!(inventory.counts(), (1, 1, 1, 0));
        assert_eq!(inventory.filtered("fire", None).len(), 1);
        assert_eq!(inventory.filtered("", Some(Source::Snap)).len(), 1);
    }
}
