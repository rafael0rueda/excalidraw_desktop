#!/bin/sh
# The Iced build as an .rpm and a .deb (docs/iced-port/PLAN.md, phase 7).
#
#   iced/packaging/package.sh            build in a Debian 12 container
#   iced/packaging/package.sh --host     build with this machine's toolchain
#
# Writes iced/target/packages/excalidraw-desktop-iced-VERSION-1.ARCH.rpm and
# excalidraw-desktop-iced_VERSION_amd64.deb. VERSION is package.json's.
#
# Why a container: a binary needs at least the glibc it was linked against.
# Built on Fedora 45 it asks for 2.44 (glibc 2.43 and 2.44 gave cosh, sinh,
# acosf and friends new symbol versions), so it would run on nothing older.
# Built on Debian 12 it asks for what that glibc has, and runs on Debian 12,
# Ubuntu 22.04 and every Fedora since 36. The packages say so either way:
# the floor is read from the binary, not written down here.
#
# Package and app sit beside the Tauri app rather than replacing it, until
# phase 8 decides: package excalidraw-desktop-iced, binary excalidraw-iced,
# config in ~/.config/excalidraw-desktop-iced. The MIME definition is the
# Tauri app's, installed under this package's name, so neither package owns
# a file of the other's.
#
# Needs: podman (or --host with cargo), rpmbuild, objdump, ar, tar, xz;
# node_modules, whose Excalidraw package the fonts come from.

set -eu
umask 022

HERE=$(cd "$(dirname "$0")" && pwd)
ICED=$(dirname "$HERE")
REPO=$(dirname "$ICED")
OUT="$ICED/target/packages"
NAME=excalidraw-desktop-iced
BIN=excalidraw-iced
SUMMARY="Local-first Excalidraw, the native Iced build"
HOMEPAGE="https://github.com/rafael0rueda/excalidraw_desktop"
IMAGE=${IMAGE:-docker.io/library/rust:1-bookworm}

die() {
    echo "package.sh: $*" >&2
    exit 1
}

VERSION=$(sed -n 's/^  "version": "\([^"]*\)",$/\1/p' "$REPO/package.json")
[ -n "$VERSION" ] || die "no version in package.json"
[ -d "$REPO/node_modules/@excalidraw/excalidraw/dist/prod/fonts" ] ||
    die "run npm install first: the bundled fonts come from node_modules"
[ "$(uname -m)" = x86_64 ] || die "only x86_64 is packaged"

# --- build

case "${1:-}" in
--host)
    cargo build --release --locked -p "$BIN" --manifest-path "$ICED/Cargo.toml"
    BUILT="$ICED/target/release/$BIN"
    ;;
"")
    # The registry in a volume of its own, so later builds do not fetch
    # every crate again. No SELinux relabelling of the checkout.
    podman run --rm --security-opt label=disable \
        -v "$REPO:/src" -v excalidraw-iced-cargo:/usr/local/cargo/registry \
        -w /src/iced -e CARGO_TARGET_DIR=/src/iced/target/container \
        "$IMAGE" cargo build --release --locked -p "$BIN"
    BUILT="$ICED/target/container/release/$BIN"
    ;;
*) die "unknown option $1" ;;
esac

GLIBC=$(objdump -T "$BUILT" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -Vu | tail -1)
[ -n "$GLIBC" ] || die "could not read the glibc the binary needs"
echo "package.sh: $BIN $VERSION needs glibc $GLIBC"

# --- the files, as installed

STAGE="$OUT/stage"
rm -rf "$STAGE"
install -Dm755 "$BUILT" "$STAGE/usr/bin/$BIN"
strip --strip-unneeded "$STAGE/usr/bin/$BIN"
install -Dm644 "$HERE/$NAME.desktop" "$STAGE/usr/share/applications/$NAME.desktop"
install -Dm644 "$REPO/packaging/excalidraw-desktop.xml" "$STAGE/usr/share/mime/packages/$NAME.xml"
ICONS="$STAGE/usr/share/icons/hicolor"
for size in 32 64 128; do
    install -Dm644 "$REPO/src-tauri/icons/${size}x${size}.png" "$ICONS/${size}x${size}/apps/$NAME.png"
done
install -Dm644 "$REPO/src-tauri/icons/128x128@2x.png" "$ICONS/256x256/apps/$NAME.png"
install -Dm644 "$REPO/src-tauri/icons/icon.png" "$ICONS/512x512/apps/$NAME.png"
install -Dm644 "$REPO/resources/icon.svg" "$ICONS/scalable/apps/$NAME.svg"

DESCRIPTION="A fully offline, themeable Excalidraw drawing application for Linux,
built natively with Iced: tabs, real files, autosave and recovery, themes,
export and a shape library, without a web view."

# --- rpm

RPMTOP="$OUT/rpmbuild"
rm -rf "$RPMTOP"
mkdir -p "$RPMTOP/SPECS"
SPEC="$RPMTOP/SPECS/$NAME.spec"
cat >"$SPEC" <<EOF
Name: $NAME
Version: $VERSION
Release: 1
Summary: $SUMMARY
License: MIT
URL: $HOMEPAGE
# The binary's own libraries (glibc at the version it was linked against,
# libgcc_s) are found by rpm. These it opens itself while running: the
# keyboard always; a display server and a GPU API as the session has them
# (it draws in software without either GPU API).
Requires: libxkbcommon.so.0()(64bit)
Recommends: libwayland-client.so.0()(64bit)
Recommends: libwayland-egl.so.1()(64bit)
Recommends: libvulkan.so.1()(64bit)
Recommends: libEGL.so.1()(64bit)
Recommends: libX11.so.6()(64bit)
Recommends: libXcursor.so.1()(64bit)
Recommends: libXi.so.6()(64bit)
Recommends: libXrandr.so.2()(64bit)
Recommends: libxkbcommon-x11.so.0()(64bit)
%global debug_package %{nil}

%description
$DESCRIPTION

%install
cp -a "$STAGE/." %{buildroot}/
install -Dm644 "$REPO/LICENSE" %{buildroot}/usr/share/licenses/%{name}/LICENSE

%post
$(sed '/^#/d' "$REPO/packaging/post-install.sh")

%postun
$(sed '/^#/d' "$REPO/packaging/post-remove.sh")

%files
/usr/bin/$BIN
/usr/share/applications/$NAME.desktop
/usr/share/mime/packages/$NAME.xml
/usr/share/icons/hicolor/*/apps/$NAME.*
%license /usr/share/licenses/%{name}/LICENSE
EOF
rpmbuild -bb --quiet --define "_topdir $RPMTOP" --define "_rpmdir $OUT" "$SPEC"
RPM="$OUT/$NAME-$VERSION-1.x86_64.rpm"
mv "$OUT/x86_64/$NAME-$VERSION-1.x86_64.rpm" "$RPM"
rmdir "$OUT/x86_64"

# --- deb: an ar archive of debian-binary, control.tar and data.tar

DEBTOP="$OUT/deb"
rm -rf "$DEBTOP"
mkdir -p "$DEBTOP/control"
cp -a "$STAGE" "$DEBTOP/data"
install -Dm644 "$REPO/LICENSE" "$DEBTOP/data/usr/share/doc/$NAME/copyright"
install -m755 "$REPO/packaging/post-install.sh" "$DEBTOP/control/postinst"
install -m755 "$REPO/packaging/post-remove.sh" "$DEBTOP/control/postrm"
(cd "$DEBTOP/data" && find . -type f | sed 's|^\./||' | LC_ALL=C sort | xargs md5sum) >"$DEBTOP/control/md5sums"
cat >"$DEBTOP/control/control" <<EOF
Package: $NAME
Version: $VERSION
Architecture: amd64
Maintainer: rafa
Installed-Size: $(du -sk "$DEBTOP/data" | cut -f1)
Depends: libc6 (>= $GLIBC), libgcc-s1, libxkbcommon0
Recommends: libwayland-client0, libwayland-egl1, libvulkan1, libegl1, libx11-6, libxcursor1, libxi6, libxrandr2, libxkbcommon-x11-0
Section: graphics
Priority: optional
Homepage: $HOMEPAGE
Description: $SUMMARY
$(echo "$DESCRIPTION" | sed 's/^/ /')
EOF
TAR="tar --owner=0 --group=0 --numeric-owner --sort=name"
$TAR -czf "$DEBTOP/control.tar.gz" -C "$DEBTOP/control" .
$TAR -cJf "$DEBTOP/data.tar.xz" -C "$DEBTOP/data" .
echo 2.0 >"$DEBTOP/debian-binary"
DEB="$OUT/${NAME}_${VERSION}_amd64.deb"
rm -f "$DEB"
(cd "$DEBTOP" && ar rc "$DEB" debian-binary control.tar.gz data.tar.xz)

echo "package.sh: wrote"
echo "  $RPM"
echo "  $DEB"
