# PKGBUILD package() runs make DESTDIR="$pkgdir" VERSION="$pkgver-$pkgrel" install after build() fills target/.
ifeq ($(strip $(VERSION)),)
$(error VERSION is required: make DESTDIR=<root> VERSION=<pkgver>-<pkgrel> install)
endif

.PHONY: install
install:
	install -Dm755 target/release/flea "$(DESTDIR)/usr/bin/flea"
	install -Dm755 target/release/flea-gui "$(DESTDIR)/usr/bin/flea-gui"
	install -Dm755 tools/flea-gio-auth "$(DESTDIR)/usr/lib/flea/flea-gio-auth"
	install -Dm755 tools/flea-udisks "$(DESTDIR)/usr/lib/flea/flea-udisks"
# The portal backend, its registration and its D-Bus activation; xdg-desktop-portal reads portals/ out of every data dir.
	install -Dm755 tools/flea-portal "$(DESTDIR)/usr/lib/flea/flea-portal"
	install -Dm644 packaging/flea.portal -t "$(DESTDIR)/usr/share/xdg-desktop-portal/portals"
	install -Dm644 packaging/org.freedesktop.impl.portal.desktop.flea.service -t "$(DESTDIR)/usr/share/dbus-1/services"
# Vendor-named like dolphin's, thunar's and nemo's: nautilus owns the plain FileManager1 service path, so pacman would refuse it.
	install -Dm755 tools/flea-filemanager1 "$(DESTDIR)/usr/lib/flea/flea-filemanager1"
	install -Dm644 packaging/com.thisisgm.flea.FileManager1.service -t "$(DESTDIR)/usr/share/dbus-1/services"
	install -Dm755 tools/flea-mpris "$(DESTDIR)/usr/lib/flea/flea-mpris"
	install -Dm644 packaging/com.thisisgm.flea.desktop -t "$(DESTDIR)/usr/share/applications"
	install -Dm644 packaging/com.thisisgm.flea.svg -t "$(DESTDIR)/usr/share/icons/hicolor/scalable/apps"
	install -Dm644 LICENSE THIRD-PARTY-LICENSES.txt -t "$(DESTDIR)/usr/share/licenses/flea"
# Issue 173: an alpm hook, not a scriptlet, prints the per-user undo commands on removal.
	install -Dm644 packaging/flea.hook -t "$(DESTDIR)/usr/share/libalpm/hooks"
# paths.rs looks for /usr/share/flea/ui/boot/shell.qml, so the UI ships as data beside the binary.
	install -Dm644 ui/qmldir ui/*.qml ui/*.js -t "$(DESTDIR)/usr/share/flea/ui"
	install -Dm644 ui/js/*.js ui/js/*.mjs -t "$(DESTDIR)/usr/share/flea/ui/js"
	install -Dm644 ui/vendor/*.mjs -t "$(DESTDIR)/usr/share/flea/ui/vendor"
	install -Dm644 ui/vendor/LICENSES/* -t "$(DESTDIR)/usr/share/flea/ui/vendor/LICENSES"
# The shell, the picker and the two tab tear-off windows, in their own directory so ui/qmldir's singletons stay off the startup path.
	install -Dm644 ui/boot/shell.qml ui/boot/picker.qml ui/boot/fleatab.qml ui/boot/tabtearoff.qml -t "$(DESTDIR)/usr/share/flea/ui/boot"
# The bar plugin, flat as src/shelfplugin.rs copies it into the user's plugin directory when the shelf is enabled.
	install -Dm644 shelf/manifest.json shelf/README.md shelf/*.qml shelf/*.js -t "$(DESTDIR)/usr/share/flea/shelf"
# Only the catalogs, which src/i18n/mod.rs reads from /usr/share/flea/i18n; the rest of i18n/ is build and test data.
	for catalog in i18n/*/flea.json; do install -Dm644 "$$catalog" "$(DESTDIR)/usr/share/flea/$$catalog" || exit 1; done
# The package's own version on the copy, so every build and rebuild replaces the user's (docs/release.md).
	sed -i "s/^  \"version\": \"[^\"]*\",\$$/  \"version\": \"$(VERSION)\",/" "$(DESTDIR)/usr/share/flea/shelf/manifest.json"
	grep -qF "  \"version\": \"$(VERSION)\"," "$(DESTDIR)/usr/share/flea/shelf/manifest.json" || { echo "flea: /usr/share/flea/shelf/manifest.json does not carry the version $(VERSION)" >&2; exit 1; }
# Omarchy's own Commons and Ui, in ui and in ui/boot, because qs.Commons resolves against the config root ui/boot launches from.
	ln -sfn /usr/share/omarchy/shell/Commons "$(DESTDIR)/usr/share/flea/ui/Commons"
	ln -sfn /usr/share/omarchy/shell/Ui "$(DESTDIR)/usr/share/flea/ui/Ui"
	ln -sfn /usr/share/omarchy/shell/Commons "$(DESTDIR)/usr/share/flea/ui/boot/Commons"
	ln -sfn /usr/share/omarchy/shell/Ui "$(DESTDIR)/usr/share/flea/ui/boot/Ui"
