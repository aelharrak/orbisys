Name:           orbisys
Version:        1.0.0
Release:        1%{?dist}
Summary:        Gestionnaire de système pour Ubuntu
License:        MIT
URL:            https://github.com/aelharrak/orbisys
Source0:        %{name}-%{version}.tar.gz
BuildRequires:  cargo, rust
Requires:       polkit

%description
Inventaire des logiciels, mises à jour, espace disque, nettoyage, processus,
services, journaux, matériel, réseau, pare-feu, sécurité, sauvegarde, noyaux,
volumes LVM/LUKS, périphériques et bureau GNOME.

%prep
%setup -q

%build
cd rust/orbisys
cargo build --release --offline

%install
install -Dm0755 rust/orbisys/target/release/orbisys %{buildroot}%{_bindir}/orbisys
install -Dm0644 rust/orbisys/packaging/orbisys.desktop %{buildroot}%{_datadir}/applications/orbisys.desktop
install -Dm0644 rust/orbisys/packaging/orbisys.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/orbisys.svg

%files
%{_bindir}/orbisys
%{_datadir}/applications/orbisys.desktop
%{_datadir}/icons/hicolor/scalable/apps/orbisys.svg

%changelog
* Mon Nov 09 2026 OrbiSys <noreply@example.com> - 1.0.0-1
- Version 1.0.0 du portage Rust.
* Mon Nov 02 2026 OrbiSys <noreply@example.com> - 0.9.0-1
- Première version du portage Rust.
