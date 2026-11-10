Name:           orbisys
Version:        1.0.0
Release:        1%{?dist}
Summary:        Linux system maintenance and administration application
License:        MIT AND ISC
URL:            https://github.com/aelharrak/orbisys
Source0:        https://github.com/aelharrak/orbisys/releases/download/v%{version}/%{name}-%{version}-vendor.tar.gz
BuildRequires:  cargo, rust
Requires:       polkit

%description
Inventaire des logiciels, mises à jour, espace disque, nettoyage, processus,
services, journaux, matériel, réseau, pare-feu, sécurité, sauvegarde, noyaux,
volumes LVM/LUKS, périphériques et bureau GNOME.

%prep
%setup -q -n %{name}-%{version}

%build
cd %{_builddir}/%{name}-%{version}
cargo build --release --locked --offline

%install
install -Dm0755 target/release/orbisys %{buildroot}%{_bindir}/orbisys
install -Dm0644 packaging/orbisys.desktop %{buildroot}%{_datadir}/applications/orbisys.desktop
install -Dm0644 packaging/orbisys.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/orbisys.svg
install -Dm0644 assets/icons/LICENSE %{buildroot}%{_licensedir}/orbisys/LICENSE.icons

%files
%{_bindir}/orbisys
%{_datadir}/applications/orbisys.desktop
%{_datadir}/icons/hicolor/scalable/apps/orbisys.svg
%license %{_licensedir}/orbisys/LICENSE.icons

%changelog
* Mon Nov 09 2026 AMINE EL HARRAK <amine.elharrak@gmail.com> - 1.0.0-1
- Version 1.0.0 du portage Rust.
* Mon Nov 02 2026 AMINE EL HARRAK <amine.elharrak@gmail.com> - 0.9.0-1
- Première version du portage Rust.
