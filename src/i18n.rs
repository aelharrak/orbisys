//! Traductions de l'interface : français, anglais, espagnol.
//!
//! Les libellés sont stockés dans une table `(clé, fr, en, es)`. Une clé
//! absente est renvoyée telle quelle, ce qui rend l'ajout de langues et de
//! libellés purement additif.

use serde::{Deserialize, Serialize};

/// Langue de l'interface.
///
/// Le français, l'anglais et l'espagnol ont leur table complète ci-dessous ; les
/// autres langues sont des fichiers `src/lang/<code>.tsv` (clé, tabulation, texte)
/// qui peuvent être partiels : une clé absente se replie sur l'anglais.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Lang {
    #[default]
    Fr,
    En,
    De,
    Es,
    Pt,
    It,
    Nl,
    Ja,
    Ko,
    Zh,
    Ar,
}

impl Lang {
    /// Ordre du sélecteur.
    pub const ALL: [Lang; 11] = [
        Lang::En,
        Lang::Fr,
        Lang::De,
        Lang::Es,
        Lang::Pt,
        Lang::It,
        Lang::Nl,
        Lang::Ja,
        Lang::Ko,
        Lang::Zh,
        Lang::Ar,
    ];

    /// Code court persisté dans les préférences.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::De => "de",
            Lang::Es => "es",
            Lang::Pt => "pt",
            Lang::It => "it",
            Lang::Nl => "nl",
            Lang::Ja => "ja",
            Lang::Ko => "ko",
            Lang::Zh => "zh",
            Lang::Ar => "ar",
        }
    }

    /// Accepte un code court (`fr`) ou régional (`pt-BR`, `zh_CN`, `en_US.UTF-8`).
    pub fn from_code(code: &str) -> Lang {
        let short: String = code
            .chars()
            .take_while(|ch| ch.is_ascii_alphabetic())
            .collect::<String>()
            .to_ascii_lowercase();
        Lang::ALL
            .iter()
            .copied()
            .find(|lang| lang.code() == short)
            .unwrap_or(Lang::Fr)
    }

    /// Nom de la langue dans cette langue (sélecteur).
    pub fn label(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::De => "Deutsch",
            Lang::Es => "Español",
            Lang::Pt => "Português",
            Lang::It => "Italiano",
            Lang::Nl => "Nederlands",
            Lang::Ja => "日本語",
            Lang::Ko => "한국어",
            Lang::Zh => "中文",
            Lang::Ar => {
                // egui ne relie pas les lettres : le nom est mis en forme visuelle.
                static NAME: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
                NAME.get_or_init(|| Box::leak(crate::arabic::shape("العربية").into_boxed_str()))
            }
        }
    }

    /// Écriture de droite à gauche ? (le texte est mis en forme, pas la disposition)
    pub fn is_rtl(self) -> bool {
        self == Lang::Ar
    }
}

/// Table `clé → texte` d'une langue à fichier (allemand, portugais…).
fn file_table(lang: Lang) -> Option<&'static std::collections::HashMap<&'static str, &'static str>> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static TABLES: OnceLock<HashMap<Lang, HashMap<&'static str, &'static str>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        let sources: [(Lang, &'static str); 8] = [
            (Lang::De, include_str!("lang/de.tsv")),
            (Lang::Pt, include_str!("lang/pt.tsv")),
            (Lang::It, include_str!("lang/it.tsv")),
            (Lang::Nl, include_str!("lang/nl.tsv")),
            (Lang::Ja, include_str!("lang/ja.tsv")),
            (Lang::Ko, include_str!("lang/ko.tsv")),
            (Lang::Zh, include_str!("lang/zh.tsv")),
            (Lang::Ar, include_str!("lang/ar.tsv")),
        ];
        sources
            .into_iter()
            .map(|(lang, text)| (lang, parse_tsv(lang, text)))
            .collect()
    });
    tables.get(&lang)
}

/// Analyse un fichier `clé<TAB>texte` (lignes vides et `#` ignorées). L'arabe est
/// mis en forme visuelle une fois pour toutes (voir `arabic`).
pub(crate) fn parse_tsv(lang: Lang, text: &'static str) -> std::collections::HashMap<&'static str, &'static str> {
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('\t'))
        .map(|(key, value)| {
            let shown: &'static str = if lang.is_rtl() || value.contains("\\n") {
                let text = value.replace("\\n", "\n");
                let text = if lang.is_rtl() { crate::arabic::shape(&text) } else { text };
                Box::leak(text.into_boxed_str())
            } else {
                value
            };
            (key, shown)
        })
        .collect()
}

/// Clés présentes dans le fichier d'une langue (tests et outils).
pub fn file_keys(lang: Lang) -> Vec<&'static str> {
    file_table(lang).map(|table| table.keys().copied().collect()).unwrap_or_default()
}

type Row = (&'static str, &'static str, &'static str, &'static str);

/// Table des traductions.
pub const TABLE: &[Row] = &[
    // Navigation
    ("nav.monitoring", "Surveillance", "Monitoring", "Supervisión"),
    ("nav.software", "Logiciels", "Software", "Software"),
    ("nav.storage", "Stockage", "Storage", "Almacenamiento"),
    ("nav.kernel", "Système & noyaux", "System & kernels", "Sistema y núcleos"),
    ("nav.volumes", "Volumes & chiffrement", "Volumes & encryption", "Volúmenes y cifrado"),
    ("nav.peripherals", "Périphériques", "Peripherals", "Periféricos"),
    ("nav.desktop", "Bureau", "Desktop", "Escritorio"),
    ("nav.automation", "Automatisation", "Automation", "Automatización"),
    ("nav.system", "Système", "System", "Sistema"),
    ("nav.network", "Réseau", "Network", "Red"),
    ("nav.security", "Sécurité", "Security", "Seguridad"),
    ("nav.locale", "Polices & locales", "Fonts & locales", "Fuentes y locales"),
    ("nav.optimize", "Optimisation", "Optimisation", "Optimización"),
    ("nav.snapshots", "Instantanés & archives", "Snapshots & archives", "Instantáneas y archivos"),
    ("nav.app", "Application", "Application", "Aplicación"),
    // Titres de pages
    ("page.dashboard", "Tableau de bord", "Dashboard", "Panel"),
    ("page.software", "Inventaire & désinstallation", "Inventory & removal", "Inventario y desinstalación"),
    ("page.updates", "Mises à jour", "Updates", "Actualizaciones"),
    ("page.disk", "Espace disque", "Disk usage", "Uso del disco"),
    ("page.storage", "Stockage & partitions", "Storage & partitions", "Almacenamiento y particiones"),
    ("page.duplicates", "Fichiers en double", "Duplicate files", "Archivos duplicados"),
    ("page.cleanup", "Nettoyage", "Cleanup", "Limpieza"),
    ("page.backup", "Sauvegarde & récupération", "Backup & recovery", "Copia y recuperación"),
    ("page.kernel", "Système & noyaux", "System & kernels", "Sistema y núcleos"),
    ("page.volumes", "Volumes & chiffrement", "Volumes & encryption", "Volúmenes y cifrado"),
    ("page.peripherals", "Imprimantes & audio", "Printers & audio", "Impresoras y audio"),
    ("page.printers", "Imprimantes avancées", "Advanced printers", "Impresoras avanzadas"),
    ("hint.printers", "Files d'attente, travaux bloqués, pilotes et partage CUPS.", "Queues, stuck jobs, drivers and CUPS sharing.", "Colas, trabajos bloqueados, controladores y compartición CUPS."),
    ("page.fonts", "Polices détaillées", "Detailed fonts", "Fuentes detalladas"),
    ("hint.fonts", "Familles installées, substitution fontconfig, cache et installation.", "Installed families, fontconfig substitution, cache and installation.", "Familias instaladas, sustitución fontconfig, caché e instalación."),
    ("page.quotas", "Quotas disque", "Disk quotas", "Cuotas de disco"),
    ("hint.quotas", "Quotas utilisateurs et groupes, seuils et activation.", "User and group quotas, thresholds and activation.", "Cuotas de usuario y grupo, umbrales y activación."),
    ("page.btrfs", "Instantanés Btrfs", "Btrfs snapshots", "Instantáneas Btrfs"),
    ("hint.btrfs", "Sous-volumes, création d'instantanés et suppression.", "Subvolumes, snapshot creation and deletion.", "Subvolúmenes, creación de instantáneas y eliminación."),
    ("page.cis", "Audit CIS", "CIS audit", "Auditoría CIS"),
    ("hint.cis", "Contrôles de durcissement, score par catégorie et remédiations.", "Hardening checks, score by category and remediation.", "Controles de endurecimiento, puntuación por categoría y remediación."),
    ("page.orphans", "Paquets orphelins", "Orphan packages", "Paquetes huérfanos"),
    ("hint.orphans", "Dépendances inutiles et configurations résiduelles à purger.", "Unneeded dependencies and residual configs to purge.", "Dependencias innecesarias y configuraciones residuales a purgar."),
    ("page.perf", "Profils de performance", "Performance profiles", "Perfiles de rendimiento"),
    ("hint.perf", "Gouvernance CPU, swap, THP et réglages de latence.", "CPU governor, swap, THP and latency settings.", "Gobernador de CPU, swap, THP y ajustes de latencia."),
    ("page.klogs", "Journaux noyau", "Kernel logs", "Registros del núcleo"),
    ("hint.klogs", "dmesg classifié : erreurs matérielles, OOM, stockage et démarrages.", "Classified dmesg: hardware errors, OOM, storage and boots.", "dmesg clasificado: errores de hardware, OOM, almacenamiento y arranques."),
    ("page.sessions", "Sessions & utilisateurs", "Sessions & users", "Sesiones y usuarios"),
    ("hint.sessions", "Sessions actives, dernières connexions et verrouillage de compte.", "Active sessions, recent logins and account locking.", "Sesiones activas, inicios recientes y bloqueo de cuentas."),
    ("page.netmounts", "Montages réseau", "Network mounts", "Montajes de red"),
    ("hint.netmounts", "NFS et CIFS : fstab, état, montage et test du serveur.", "NFS and CIFS: fstab, state, mounting and server probe.", "NFS y CIFS: fstab, estado, montaje y prueba del servidor."),
    ("page.timers", "Sauvegardes systemd", "systemd backups", "Copias systemd"),
    ("hint.timers", "Timers, unités de sauvegarde, dernières exécutions et échecs.", "Timers, backup units, recent runs and failures.", "Temporizadores, unidades de copia, ejecuciones y fallos."),
    ("page.desktop", "Bureau GNOME", "GNOME desktop", "Escritorio GNOME"),
    ("page.automation", "Automatisation", "Automation", "Automatización"),
    ("page.processes", "Processus", "Processes", "Procesos"),
    ("page.services", "Services", "Services", "Servicios"),
    ("page.logs", "Journaux", "Logs", "Registros"),
    ("page.logadvanced", "Journaux avancés", "Advanced logs", "Registros avanzados"),
    ("page.customunits", "Services personnalisés", "Custom services", "Servicios personalizados"),
    ("page.hardware", "Matériel", "Hardware", "Hardware"),
    ("page.network", "Réseau & partage", "Network & sharing", "Red y compartición"),
    ("page.netadvanced", "Réseau étendu", "Advanced network", "Red avanzada"),
    ("page.firewall", "Pare-feu & VPN", "Firewall & VPN", "Cortafuegos y VPN"),
    ("page.security", "Audit de sécurité", "Security audit", "Auditoría de seguridad"),
    ("page.reports", "Rapports", "Reports", "Informes"),
    ("page.locale", "Polices & locales", "Fonts & locales", "Fuentes y locales"),
    ("page.optimize", "Optimisation", "Optimisation", "Optimización"),
    ("page.snapshots", "Instantanés & archives", "Snapshots & archives", "Instantáneas y archivos"),
    ("page.about", "À propos", "About", "Acerca de"),
    // Sous-titres
    ("hint.dashboard", "Vue d'ensemble de la machine, rafraîchie automatiquement.", "Machine overview, refreshed automatically.", "Vista general del equipo, actualizada automáticamente."),
    ("hint.software", "Tout ce qui est installé, toutes sources confondues.", "Everything installed, across all sources.", "Todo lo instalado, de todas las fuentes."),
    ("hint.updates", "Correctifs disponibles via le gestionnaire natif.", "Updates available through the native manager.", "Actualizaciones disponibles en el gestor nativo."),
    ("hint.disk", "Occupation des systèmes de fichiers montés.", "Usage of mounted filesystems.", "Uso de los sistemas de archivos montados."),
    ("hint.storage", "Appareils, montages, Btrfs et instantanés Timeshift.", "Devices, mounts, Btrfs and Timeshift snapshots.", "Dispositivos, montajes, Btrfs e instantáneas."),
    ("hint.duplicates", "Fichiers identiques, comparés par empreinte.", "Identical files compared by checksum.", "Archivos idénticos comparados por huella."),
    ("hint.cleanup", "Cibles de nettoyage mesurées avant toute suppression.", "Cleanup targets measured before any removal.", "Objetivos de limpieza medidos antes de borrar."),
    ("hint.backup", "Liste des logiciels, copie rsync, dotfiles et GRUB.", "Software list, rsync copy, dotfiles and GRUB.", "Lista de software, copia rsync, dotfiles y GRUB."),
    ("hint.kernel", "Paramètres noyau, swap, noyaux installés et pilotes.", "Kernel settings, swap, installed kernels and drivers.", "Ajustes del núcleo, swap, núcleos y controladores."),
    ("hint.volumes", "Groupes et volumes LVM, périphériques LUKS, montage.", "LVM groups and volumes, LUKS devices, mounting.", "Grupos y volúmenes LVM, dispositivos LUKS, montaje."),
    ("hint.peripherals", "Files d'impression CUPS, sorties et entrées audio.", "CUPS print queues, audio outputs and inputs.", "Colas CUPS, salidas y entradas de audio."),
    ("hint.desktop", "Thème, icônes, police, extensions et raccourcis.", "Theme, icons, font, extensions and shortcuts.", "Tema, iconos, fuente, extensiones y atajos."),
    ("hint.automation", "Tâches cron et minuteurs systemd.", "Cron jobs and systemd timers.", "Tareas cron y temporizadores systemd."),
    ("hint.processes", "Processus triés par consommation CPU.", "Processes sorted by CPU usage.", "Procesos ordenados por uso de CPU."),
    ("hint.services", "Unités systemd et actions confirmées.", "systemd units and confirmed actions.", "Unidades systemd y acciones confirmadas."),
    ("hint.logs", "Journal systemd filtré par unité et par niveau.", "systemd journal filtered by unit and level.", "Registro systemd filtrado por unidad y nivel."),
    ("hint.logadvanced", "Recherche multi-unités, plage de dates et erreurs récurrentes.", "Multi-unit search, date range and recurring errors.", "Búsqueda multiunidad, rango de fechas y errores recurrentes."),
    ("hint.customunits", "Écrire, installer et piloter ses propres unités systemd.", "Write, install and control your own systemd units.", "Escribir, instalar y controlar sus propias unidades systemd."),
    ("hint.hardware", "Processeur, mémoire, cartes, batterie et capteurs.", "CPU, memory, cards, battery and sensors.", "CPU, memoria, tarjetas, batería y sensores."),
    ("hint.network", "Interfaces, Wi-Fi, DNS, SSH et partages.", "Interfaces, Wi-Fi, DNS, SSH and shares.", "Interfaces, Wi-Fi, DNS, SSH y recursos."),
    ("hint.netadvanced", "Routes, DNS, VPN, Samba et diagnostic de connectivité.", "Routes, DNS, VPN, Samba and connectivity diagnostics.", "Rutas, DNS, VPN, Samba y diagnóstico de conectividad."),
    ("hint.firewall", "Règles UFW et tunnels VPN.", "UFW rules and VPN tunnels.", "Reglas UFW y túneles VPN."),
    ("hint.security", "Contrôles en lecture seule, sans modification.", "Read-only checks, nothing is changed.", "Comprobaciones de solo lectura, sin cambios."),
    ("hint.reports", "Exports TXT, CSV, JSON et PDF.", "TXT, CSV, JSON and PDF exports.", "Exportaciones TXT, CSV, JSON y PDF."),
    ("hint.locale", "Langue du système, fuseau horaire et polices.", "System language, time zone and fonts.", "Idioma del sistema, zona horaria y fuentes."),
    ("hint.optimize", "TRIM, gouvernance CPU, mémoire et démarrage.", "TRIM, CPU governor, memory and boot.", "TRIM, gobernador de CPU, memoria y arranque."),
    ("hint.snapshots", "Timeshift, sauvegarde distante et vérification d'archives.", "Timeshift, remote backup and archive verification.", "Timeshift, copia remota y verificación de archivos."),
    ("hint.about", "Version, outils détectés, licence.", "Version, detected tools, licence.", "Versión, herramientas detectadas, licencia."),
    // Interface générale
    ("app.search", "Rechercher…", "Search…", "Buscar…"),
    ("group.processes", "Processus & surveillance", "Processes & monitoring", "Procesos y monitorización"),
    ("group.reports", "Rapports & historique", "Reports & history", "Informes e historial"),
    ("group.disks", "Disques", "Disks", "Discos"),
    ("group.backups", "Sauvegardes", "Backups", "Copias de seguridad"),
    ("group.audit", "Audit & intégrité", "Audit & integrity", "Auditoría e integridad"),
    ("group.keys", "Clés & certificats", "Keys & certificates", "Claves y certificados"),
    ("group.protections", "Protections système", "System protections", "Protecciones del sistema"),
    ("group.packages", "Paquets & mises à jour", "Packages & updates", "Paquetes y actualizaciones"),
    ("group.peripherals", "Périphériques", "Peripherals", "Periféricos"),
    ("app.refresh", "Rafraîchir", "Refresh", "Actualizar"),
    ("app.theme", "Thème", "Theme", "Tema"),
    ("app.search_hint", "Recherche globale : Ctrl+K", "Global search: Ctrl+K", "Búsqueda global: Ctrl+K"),
    ("app.busy", "lecture en cours", "loading", "cargando"),
    ("app.version", "Version", "Version", "Versión"),
    // À propos
    ("about.distribution", "Distribution", "Distribution", "Distribución"),
    ("about.kernel", "Noyau", "Kernel", "Núcleo"),
    ("about.manager", "Gestionnaire", "Manager", "Gestor"),
    ("about.native", "Paquets natifs", "Native packages", "Paquetes nativos"),
    ("about.accent", "Couleur d'accent", "Accent colour", "Color de acento"),
    ("about.language", "Langue", "Language", "Idioma"),
    ("about.missing", "Outils optionnels absents", "Optional tools missing", "Herramientas opcionales ausentes"),
    ("about.all_present", "Tous les outils optionnels sont présents.", "All optional tools are present.", "Todas las herramientas opcionales están presentes."),
    ("about.policy", "Les actions privilégiées passent par Polkit ; aucune commande n'est exécutée via un shell.", "Privileged actions go through Polkit; no command runs through a shell.", "Las acciones privilegiadas usan Polkit; ningún comando se ejecuta mediante un shell."),
    ("about.active", "actif", "active", "activo"),
    ("about.available", "disponible", "available", "disponible"),
    ("about.missing_short", "absent", "missing", "ausente"),
    // Rubriques génériques
    ("page.monitor", "Surveillance en direct", "Live monitoring", "Supervisión en vivo"),
    ("hint.monitor", "Processeur, mémoire, réseau et disques, actualisés en continu.", "CPU, memory, network and disks, refreshed continuously.", "CPU, memoria, red y discos, actualizados de forma continua."),
    ("page.alerts", "Alertes", "Alerts", "Alertas"),
    ("hint.alerts", "Seuils processeur, mémoire, disque et température avec notification.", "CPU, memory, disk and temperature thresholds with notification.", "Umbrales de CPU, memoria, disco y temperatura con notificación."),
    ("page.health", "Santé du système", "System health", "Salud del sistema"),
    ("hint.health", "Score global calculé à partir de contrôles et export du rapport.", "Overall score computed from checks, with report export.", "Puntuación global a partir de comprobaciones, con informe exportable."),
    ("page.history", "Historique & tendances", "History & trends", "Historial y tendencias"),
    ("hint.history", "Mesures dans le temps, courbes et export CSV.", "Measurements over time, curves and CSV export.", "Mediciones en el tiempo, curvas y exportación CSV."),
    ("page.delivery", "Envoi des rapports", "Report delivery", "Envío de informes"),
    ("hint.delivery", "Livraison du rapport d'état par webhook ou e-mail.", "Deliver the status report by webhook or e-mail.", "Entrega del informe de estado por webhook o correo."),
    ("page.benchmark", "Benchmark", "Benchmark", "Benchmark"),
    ("hint.benchmark", "Mesures simples : processeur, mémoire et disque.", "Simple measurements: CPU, memory and disk.", "Mediciones simples: CPU, memoria y disco."),
    ("page.aptrepos", "Dépôts APT", "APT repositories", "Repositorios APT"),
    ("hint.aptrepos", "Sources de paquets : activer, désactiver, supprimer, mettre à jour les index.", "Package sources: enable, disable, remove, refresh indexes.", "Fuentes de paquetes: activar, desactivar, eliminar, actualizar índices."),
    ("page.advpackages", "Paquets avancés", "Advanced packages", "Paquetes avanzados"),
    ("hint.advpackages", "Verrouillage de version, rétrogradation et historique dpkg.", "Version holds, downgrades and dpkg history.", "Bloqueo de versión, degradación e historial de dpkg."),
    ("page.runtimes", "Runtimes & langages", "Runtimes & languages", "Runtimes y lenguajes"),
    ("hint.runtimes", "Versions des langages installés et alternatives système.", "Installed language versions and system alternatives.", "Versiones de lenguajes instalados y alternativas del sistema."),
    ("page.containers", "Conteneurs", "Containers", "Contenedores"),
    ("hint.containers", "Docker, Podman et instances LXD : démarrer, arrêter, supprimer.", "Docker, Podman and LXD instances: start, stop, remove.", "Docker, Podman e instancias LXD: iniciar, detener, eliminar."),
    ("page.containersadv", "Conteneurs avancés", "Advanced containers", "Contenedores avanzados"),
    ("hint.containersadv", "Images, volumes et réseaux avec nettoyage ciblé.", "Images, volumes and networks with targeted cleanup.", "Imágenes, volúmenes y redes con limpieza dirigida."),
    ("page.startup", "Démarrage", "Startup", "Inicio"),
    ("hint.startup", "Applications de session, profil d'alimentation et analyse du démarrage.", "Session apps, power profile and boot analysis.", "Aplicaciones de sesión, perfil de energía y análisis del arranque."),
    ("page.actionlog", "Journal des actions", "Action log", "Registro de acciones"),
    ("hint.actionlog", "Historique horodaté des opérations, export CSV et purge.", "Timestamped history of operations, CSV export and purge.", "Historial de operaciones con fecha, exportación CSV y purga."),
    ("page.maintenance", "Maintenance", "Maintenance", "Mantenimiento"),
    ("hint.maintenance", "Outils disponibles et tâches privilégiées d'entretien.", "Available tools and privileged maintenance tasks.", "Herramientas disponibles y tareas de mantenimiento privilegiadas."),
    ("page.troubleshoot", "Dépannage", "Troubleshooting", "Solución de problemas"),
    ("hint.troubleshoot", "Diagnostic automatique et conseils.", "Automatic diagnosis and advice.", "Diagnóstico automático y consejos."),
    ("page.profiles", "Profils", "Profiles", "Perfiles"),
    ("hint.profiles", "Profils de poste prêts à l'emploi avec installation groupée.", "Ready-made workstation profiles with bulk install.", "Perfiles de equipo listos con instalación en lote."),
    ("page.users", "Utilisateurs", "Users", "Usuarios"),
    ("hint.users", "Comptes locaux, groupe sudo et verrouillage.", "Local accounts, sudo group and locking.", "Cuentas locales, grupo sudo y bloqueo."),
    ("page.advcleanup", "Nettoyage avancé", "Advanced cleanup", "Limpieza avanzada"),
    ("hint.advcleanup", "Caches applicatifs, Flatpak, conteneurs et anciennes révisions Snap.", "App caches, Flatpak, containers and old Snap revisions.", "Cachés de aplicaciones, Flatpak, contenedores y revisiones Snap antiguas."),
    ("page.mounts", "Montages", "Mounts", "Montajes"),
    ("hint.mounts", "Points de montage actifs, /etc/fstab et TRIM des SSD.", "Active mount points, /etc/fstab and SSD TRIM.", "Puntos de montaje activos, /etc/fstab y TRIM de SSD."),
    ("page.usbboot", "Clé USB bootable", "Bootable USB", "USB de arranque"),
    ("hint.usbboot", "Écriture d'une image ISO sur un périphérique amovible.", "Write an ISO image to a removable device.", "Escritura de una imagen ISO en un dispositivo extraíble."),
    ("page.archives", "Archives", "Archives", "Archivos comprimidos"),
    ("hint.archives", "Recensement, création et extraction d'archives.", "List, create and extract archives.", "Listado, creación y extracción de archivos."),
    ("page.traffic", "Trafic réseau", "Network traffic", "Tráfico de red"),
    ("hint.traffic", "Connexions par processus et compteurs par interface.", "Connections per process and per-interface counters.", "Conexiones por proceso y contadores por interfaz."),
    ("page.lan", "Réseau local", "Local network", "Red local"),
    ("hint.lan", "Découverte des hôtes du réseau (nmap ou table ARP).", "Host discovery on the network (nmap or ARP table).", "Descubrimiento de equipos en la red (nmap o tabla ARP)."),
    ("page.sshgpg", "Clés SSH & GPG", "SSH & GPG keys", "Claves SSH y GPG"),
    ("hint.sshgpg", "Clés de l'utilisateur, génération SSH et copie de la clé publique.", "User keys, SSH generation and public key copy.", "Claves del usuario, generación SSH y copia de la clave pública."),
    ("page.certs", "Certificats & CA", "Certificates & CAs", "Certificados y CA"),
    ("hint.certs", "Autorités locales : ajout, suppression et dates d'expiration.", "Local authorities: add, remove and expiry dates.", "Autoridades locales: añadir, eliminar y fechas de caducidad."),
    ("page.secureboot", "Secure Boot", "Secure Boot", "Secure Boot"),
    ("hint.secureboot", "État de Secure Boot et clés MOK enregistrées.", "Secure Boot state and enrolled MOK keys.", "Estado de Secure Boot y claves MOK registradas."),
    ("page.apparmor", "AppArmor / SELinux", "AppArmor / SELinux", "AppArmor / SELinux"),
    ("hint.apparmor", "Profils chargés, bascule complain/enforce et mode SELinux.", "Loaded profiles, complain/enforce switch and SELinux mode.", "Perfiles cargados, cambio complain/enforce y modo SELinux."),
    ("page.integrity", "Intégrité", "Integrity", "Integridad"),
    ("hint.integrity", "Vérification des fichiers de paquets modifiés (dpkg -V, debsums).", "Check modified package files (dpkg -V, debsums).", "Verificación de archivos de paquetes modificados (dpkg -V, debsums)."),
    ("page.acl", "ACL & permissions", "ACLs & permissions", "ACL y permisos"),
    ("hint.acl", "Analyse des droits et gestion des listes de contrôle d'accès.", "Inspect permissions and manage access control lists.", "Análisis de permisos y gestión de listas de control de acceso."),
    ("page.bluetooth", "Bluetooth & USB", "Bluetooth & USB", "Bluetooth y USB"),
    ("hint.bluetooth", "Appareils appairés, connexion et périphériques USB.", "Paired devices, connection and USB devices.", "Dispositivos emparejados, conexión y dispositivos USB."),
    ("page.power", "Énergie & batterie", "Power & battery", "Energía y batería"),
    ("hint.power", "Batterie, cycles de charge et profil d'alimentation.", "Battery, charge cycles and power profile.", "Batería, ciclos de carga y perfil de energía."),
    ("page.fans", "Ventilateurs & capteurs", "Fans & sensors", "Ventiladores y sensores"),
    ("hint.fans", "Vitesses des ventilateurs et températures (lecture seule).", "Fan speeds and temperatures (read-only).", "Velocidad de ventiladores y temperaturas (solo lectura)."),
    ("page.cloudbackup", "Sauvegarde cloud", "Cloud backup", "Copia en la nube"),
    ("hint.cloudbackup", "Synchronisation de dossiers vers un remote rclone, avec simulation.", "Sync folders to an rclone remote, with dry-run.", "Sincronización de carpetas con un remote rclone, con simulación."),
    ("page.schedule", "Rapports planifiés", "Scheduled reports", "Informes programados"),
    ("hint.schedule", "Minuterie systemd utilisateur : rapport quotidien, hebdomadaire ou mensuel.", "User systemd timer: daily, weekly or monthly report.", "Temporizador systemd de usuario: informe diario, semanal o mensual."),
    ("page.wifivpn", "Wi-Fi & VPN", "Wi-Fi & VPN", "Wi-Fi y VPN"),
    ("hint.wifivpn", "Radio Wi-Fi, réseaux détectés, connexions VPN et import de configuration.", "Wi-Fi radio, detected networks, VPN connections and config import.", "Radio Wi-Fi, redes detectadas, conexiones VPN e importación de configuración."),
    ("page.firmware", "Firmware", "Firmware", "Firmware"),
    ("hint.firmware", "Mises à jour de firmware via fwupd.", "Firmware updates through fwupd.", "Actualizaciones de firmware mediante fwupd."),
    ("page.btrfsmaint", "Btrfs : entretien", "Btrfs maintenance", "Btrfs: mantenimiento"),
    ("hint.btrfsmaint", "Scrub, usage et équilibrage des systèmes de fichiers btrfs.", "Scrub, usage and balance of btrfs filesystems.", "Scrub, uso y equilibrado de sistemas de archivos btrfs."),
    ("page.dotfiles", "Dotfiles", "Dotfiles", "Dotfiles"),
    ("hint.dotfiles", "Sauvegarde et restauration des fichiers de configuration personnels.", "Back up and restore personal configuration files.", "Copia y restauración de archivos de configuración personales."),
    // Interface des rubriques génériques
    ("ui.loading", "Lecture en cours…", "Loading…", "Leyendo…"),
    ("ui.filter", "filtrer", "filter", "filtrar"),
    ("ui.reload", "Actualiser", "Reload", "Actualizar"),
    ("ui.empty", "Aucun élément.", "Nothing to show.", "Nada que mostrar."),
    ("ui.nomatch", "Aucun résultat pour ce filtre.", "No result for this filter.", "Sin resultados para este filtro."),
    ("ui.close", "Fermer", "Close", "Cerrar"),
    ("page.scan", "Scan des comportements anormaux", "Abnormal behaviour scan", "Análisis de comportamientos anómalos"),
    ("hint.scan", "Analyse heuristique des processus, du réseau, des démarrages et des fichiers pour repérer ce qui sort de l'ordinaire.", "Heuristic analysis of processes, network, startup items and files to spot what looks unusual.", "Análisis heurístico de procesos, red, inicios y archivos para detectar lo que sale de lo normal."),
    ("ui.charts", "Graphiques", "Charts", "Gráficos"),
    ("ui.select_process_hint", "Sélectionnez un processus dans la liste pour suivre son processeur et sa mémoire.", "Select a process in the list to follow its CPU and memory.", "Seleccione un proceso de la lista para seguir su procesador y memoria."),
    ("about.font_missing", "Aucune police compatible trouvée pour cette langue : installez « fonts-noto-cjk » (japonais, coréen, chinois) ou « fonts-dejavu » (arabe).", "No compatible font found for this language: install \"fonts-noto-cjk\" (Japanese, Korean, Chinese) or \"fonts-dejavu\" (Arabic).", "No se encontró ninguna fuente compatible para este idioma: instale «fonts-noto-cjk» (japonés, coreano, chino) o «fonts-dejavu» (árabe)."),
];

/// Traduit une clé ; la clé est renvoyée si elle est inconnue.
pub fn t(lang: Lang, key: &str) -> &'static str {
    resolve(file_table(lang), lang, key)
}

/// Cherche `key` dans le fichier de la langue (s'il y en a un), puis dans la table
/// intégrée ; une langue à fichier sans cette clé se replie sur l'anglais.
fn resolve(
    file: Option<&'static std::collections::HashMap<&'static str, &'static str>>,
    lang: Lang,
    key: &str,
) -> &'static str {
    if let Some(text) = file.and_then(|table| table.get(key)) {
        return text;
    }
    for (row_key, fr, en, es) in TABLE {
        if *row_key == key {
            return match lang {
                Lang::Fr => fr,
                Lang::Es => es,
                _ => en,
            };
        }
    }
    // `key` n'est pas 'static : on renvoie un libellé neutre plutôt que de
    // fuir de la mémoire.
    match lang {
        Lang::Fr => "(non traduit)",
        Lang::Es => "(sin traducir)",
        _ => "(untranslated)",
    }
}

/// Vrai si la clé existe dans la table.
pub fn has(key: &str) -> bool {
    TABLE.iter().any(|row| row.0 == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_known_keys() {
        assert_eq!(t(Lang::Fr, "page.dashboard"), "Tableau de bord");
        assert_eq!(t(Lang::En, "page.dashboard"), "Dashboard");
        assert_eq!(t(Lang::Es, "page.dashboard"), "Panel");
    }

    #[test]
    fn round_trips_language_codes() {
        for lang in Lang::ALL {
            assert_eq!(Lang::from_code(lang.code()), lang);
        }
        assert_eq!(Lang::from_code("inconnu"), Lang::Fr);
    }

    #[test]
    fn every_row_is_complete() {
        for (key, fr, en, es) in TABLE {
            assert!(has(key));
            assert!(!fr.is_empty(), "français manquant pour {key}");
            assert!(!en.is_empty(), "anglais manquant pour {key}");
            assert!(!es.is_empty(), "espagnol manquant pour {key}");
        }
    }

    #[test]
    fn codes_accept_regional_variants() {
        assert_eq!(Lang::from_code("pt-BR"), Lang::Pt);
        assert_eq!(Lang::from_code("zh_CN.UTF-8"), Lang::Zh);
        assert_eq!(Lang::from_code("de"), Lang::De);
        assert_eq!(Lang::from_code("AR"), Lang::Ar);
        assert_eq!(Lang::from_code(""), Lang::Fr);
        assert_eq!(Lang::ALL.len(), 11);
    }

    #[test]
    fn file_languages_cover_every_key() {
        // Chaque langue à fichier doit couvrir toutes les clés de la table.
        let needed: Vec<&str> = TABLE.iter().map(|row| row.0).collect();
        for lang in Lang::ALL {
            if file_table(lang).is_none() {
                continue;
            }
            let keys: std::collections::HashSet<&str> = file_keys(lang).into_iter().collect();
            for key in &needed {
                assert!(keys.contains(key), "{} : clé manquante {key}", lang.code());
                assert!(!t(lang, key).trim().is_empty());
            }
            for key in &keys {
                assert!(has(key), "{} : clé inconnue {key}", lang.code());
            }
        }
    }

    #[test]
    fn file_languages_translate_and_fall_back_to_english() {
        assert_eq!(t(Lang::De, "page.dashboard"), "Übersicht");
        assert_eq!(t(Lang::Ja, "nav.network"), "ネットワーク");
        assert_eq!(t(Lang::De, "hint.dashboard"), "Überblick über den Rechner, automatisch aktualisiert.");
        assert_ne!(t(Lang::De, "hint.dashboard"), t(Lang::En, "hint.dashboard"));
        // Une clé absente du fichier retombe sur l'anglais.
        let empty: &'static std::collections::HashMap<&'static str, &'static str> =
            Box::leak(Box::new(std::collections::HashMap::new()));
        assert_eq!(resolve(Some(empty), Lang::De, "page.dashboard"), t(Lang::En, "page.dashboard"));
        assert_eq!(resolve(Some(empty), Lang::Ja, "hint.about"), t(Lang::En, "hint.about"));
        assert_eq!(t(Lang::Zh, "app.refresh"), "刷新");
    }

    #[test]
    fn arabic_is_shaped_at_load_time() {
        let shaped = t(Lang::Ar, "ui.close");
        assert!(!shaped.contains('\u{0625}'), "les lettres de base ont été remplacées par des formes de présentation");
        assert!(shaped.chars().any(|ch| ('\u{FE70}'..='\u{FEFF}').contains(&ch)));
    }

    #[test]
    fn language_names_are_displayable() {
        for lang in Lang::ALL {
            assert!(!lang.label().is_empty());
        }
        // Le nom arabe est en formes de présentation, pas en lettres de base.
        assert!(Lang::Ar.label().chars().all(|ch| !('\u{0627}'..='\u{064A}').contains(&ch)));
    }

    #[test]
    fn unknown_key_does_not_leak() {
        assert_eq!(t(Lang::Fr, "cle.inexistante"), "(non traduit)");
        assert!(!has("cle.inexistante"));
    }
}
