# Maintainer: GM <gianmarcomorales@icloud.com>

pkgname=flea
pkgver=0.1.4
pkgrel=1
pkgdesc='Fast, keyboard-first file manager for Omarchy'
arch=('x86_64' 'aarch64')
license=('MIT')
# omarchy owns /usr/share/omarchy/shell, which ui/Commons and ui/Ui link into; quickshell owns qs.
# util-linux ships prlimit, which the thumbnail and archive sandboxes require alongside bubblewrap.
# xdg-terminal-exec is what --terminal execs, so the topbar's terminal button needs it installed.
# wl-clipboard ships wl-copy, which ui/Opener.qml pipes into for the menu's Copy Path row; nothing
# else in this closure requires it, so on a clean Omarchy box that row failed silently without it.
# python-gobject is what tools/flea-portal answers org.freedesktop.impl.portal.FileChooser with; it
# is the same binding omarchy-file-select, the client that portal serves, is already written against.
depends=('bubblewrap' 'expect' 'glib2' 'gvfs' 'gvfs-dnssd' 'gvfs-nfs' 'gvfs-smb' 'omarchy' 'python-gobject' 'quickshell' 'shared-mime-info' 'util-linux' 'wl-clipboard' 'xdg-terminal-exec' 'xdg-utils')
makedepends=('cargo')
# check() runs the real sandboxed child, which shells to /usr/bin/python3 to reserve address space.
checkdepends=('python')
optdepends=('libarchive: archive listing and extraction'
            '7zip: 7z archive support'
            'imagemagick: image conversion'
            'tailscale: Taildrop sharing')
# The release profile strips, so a debug package would have nothing to hold.
options=('!debug')
# Empty on purpose: with no source array makepkg builds from $startdir, so a clone is the source.
source=()

build() {
  # Its own target directory, so a makepkg run never disturbs the checkout's target/.
  export CARGO_TARGET_DIR="$srcdir/target"
  cd "$startdir"
  cargo build --release --locked
}

check() {
  export CARGO_TARGET_DIR="$srcdir/target"
  cd "$startdir"
  cargo test --release --locked
  # These two need no built binary and locate themselves, so they run correctly under makepkg.
  # The rest of tests/ resolves ./target/<profile>/flea against the repo root, which CARGO_TARGET_DIR
  # has moved, so they would refuse on a clean clone or silently test a stale binary on a dev box.
  ./tests/js.sh
  ./tests/keymap-gen.sh
}

package() {
  cd "$startdir"
  install -Dm755 "$srcdir/target/release/flea" "$pkgdir/usr/bin/flea"
  install -Dm755 tools/flea-gio-auth "$pkgdir/usr/lib/flea/flea-gio-auth"
  # The portal backend, its registration and its D-Bus activation: xdg-desktop-portal 1.22 reads
  # portals/ out of every data dir, and this is Flea's own package writing Flea's own files.
  install -Dm755 tools/flea-portal "$pkgdir/usr/lib/flea/flea-portal"
  install -Dm644 packaging/flea.portal -t "$pkgdir/usr/share/xdg-desktop-portal/portals"
  install -Dm644 packaging/org.freedesktop.impl.portal.desktop.flea.service -t "$pkgdir/usr/share/dbus-1/services"
  install -Dm644 packaging/com.thisisgm.flea.desktop -t "$pkgdir/usr/share/applications"
  install -Dm644 packaging/com.thisisgm.flea.svg -t "$pkgdir/usr/share/icons/hicolor/scalable/apps"
  install -Dm644 LICENSE -t "$pkgdir/usr/share/licenses/$pkgname"

  # paths.rs looks for /usr/share/flea/ui/shell.qml, so the UI ships as data beside the binary.
  install -Dm644 ui/qmldir ui/*.qml -t "$pkgdir/usr/share/flea/ui"
  install -Dm644 ui/js/*.js -t "$pkgdir/usr/share/flea/ui/js"
  # Commons and Ui are Omarchy's own, reached as qs.Commons: the checkout links them and so does the package.
  ln -s /usr/share/omarchy/shell/Commons "$pkgdir/usr/share/flea/ui/Commons"
  ln -s /usr/share/omarchy/shell/Ui "$pkgdir/usr/share/flea/ui/Ui"
}
