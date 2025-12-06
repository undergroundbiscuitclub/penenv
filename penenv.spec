Name:           penenv
Version:        0.1.0
Release:        1%{?dist}
Summary:        Pentesting environment with integrated shells and note-taking

License:        MIT
URL:            https://github.com/undergroundbiscuitclub/penenv
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  vte291-gtk4-devel

Requires:       gtk4
Requires:       libadwaita
Requires:       vte291-gtk4
Requires:       bash

%description
PenEnv is a modern GTK4 desktop application for managing penetration testing
environments with integrated shells, note-taking, and target management.
Features multiple shell tabs with full bash functionality, markdown notes
with syntax highlighting, and automatic command logging.

%prep
%setup -q

%build
cargo build --release

%install
rm -rf $RPM_BUILD_ROOT
mkdir -p $RPM_BUILD_ROOT%{_bindir}
mkdir -p $RPM_BUILD_ROOT%{_datadir}/applications
mkdir -p $RPM_BUILD_ROOT%{_datadir}/icons/hicolor/256x256/apps
mkdir -p $RPM_BUILD_ROOT%{_datadir}/icons/hicolor/scalable/apps

install -m 755 target/release/penenv $RPM_BUILD_ROOT%{_bindir}/penenv
install -m 644 penenv.desktop $RPM_BUILD_ROOT%{_datadir}/applications/penenv.desktop
install -m 644 images/penenv-icon.png $RPM_BUILD_ROOT%{_datadir}/icons/hicolor/256x256/apps/penenv.png
install -m 644 images/penenv-icon.svg $RPM_BUILD_ROOT%{_datadir}/icons/hicolor/scalable/apps/penenv.svg

%files
%license LICENSE
%doc README.md
%{_bindir}/penenv
%{_datadir}/applications/penenv.desktop
%{_datadir}/icons/hicolor/256x256/apps/penenv.png
%{_datadir}/icons/hicolor/scalable/apps/penenv.svg

%post
gtk-update-icon-cache -f -t %{_datadir}/icons/hicolor 2>/dev/null || :
update-desktop-database %{_datadir}/applications 2>/dev/null || :

%postun
gtk-update-icon-cache -f -t %{_datadir}/icons/hicolor 2>/dev/null || :
update-desktop-database %{_datadir}/applications 2>/dev/null || :

%changelog
* Thu Dec 05 2024 undergroundbiscuitclub <noreply@example.com> - 0.1.0-1
- Initial RPM release
- GTK4 desktop application
- Multiple shell tabs with full bash functionality
- Markdown notes with syntax highlighting
- Command logging with automatic refresh
- Target management with popup selector
