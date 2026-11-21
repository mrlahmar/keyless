#!/bin/sh
# Builds the Linux release artifacts into dist/:
#   keyless_<version>_amd64.deb, Keyless-<version>-x86_64.AppImage, SHA256SUMS-linux.txt
# Used by the release workflow and for local testing. Needs dpkg-deb; the AppImage also
# needs appimagetool (set APPIMAGETOOL to its path) and is skipped without it.
# Build on the oldest distro you support: the binary needs that glibc or newer.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

version=$(cargo metadata --format-version 1 --no-deps |
    sed -n 's/.*"name":"keyless","version":"\([^"]*\)".*/\1/p')
[ -n "$version" ] || { echo "cannot read the version" >&2; exit 1; }
echo "Keyless $version"

cargo build --release --locked -p keyless

stage=target/package-linux
dist=dist
rm -rf "$stage" && mkdir -p "$stage" "$dist"

# Files installed under /usr, shared by both packages. The binary lives in
# /usr/lib/keyless next to its license list (the About page opens it from there).
usr="$stage/usr"
install -Dm755 target/release/keyless "$usr/lib/keyless/keyless"
install -Dm644 LICENSE README.md packaging/licenses/* assets/fonts/OFL-*.txt -t "$usr/lib/keyless/"
if command -v cargo-about >/dev/null; then
    cargo about generate --fail packaging/about.hbs -o "$usr/lib/keyless/THIRD_PARTY_LICENSES.html"
else
    echo "warning: cargo-about not installed: skipping THIRD_PARTY_LICENSES.html" >&2
fi
mkdir -p "$usr/bin" && ln -s ../lib/keyless/keyless "$usr/bin/keyless"
install -Dm644 packaging/linux/keyless.desktop -t "$usr/share/applications/"
install -Dm644 assets/keyless.svg -t "$usr/share/icons/hicolor/scalable/apps/"
install -Dm644 packaging/linux/70-keyless.rules -t "$usr/lib/udev/rules.d/"

# AppStream metadata for software centers, with one <release> per version in CHANGELOG.md.
metainfo="$usr/share/metainfo/io.github.mrlahmar.Keyless.metainfo.xml"
mkdir -p "$(dirname "$metainfo")"
releases=$(sed -n 's/^## \[\([0-9][^]]*\)\] - \([0-9-]*\).*/\1 \2/p' CHANGELOG.md |
    while read -r v d; do
        printf '    <release version="%s" date="%s">\n' "$v" "$d"
        printf '      <url type="details">https://github.com/mrlahmar/keyless/releases/tag/v%s</url>\n' "$v"
        printf '    </release>\n'
    done)
awk -v r="$releases" '$0 == "@RELEASES@" { print r; next } { print }' \
    packaging/linux/io.github.mrlahmar.Keyless.metainfo.xml > "$metainfo"
chmod 644 "$metainfo"
if command -v appstreamcli >/dev/null; then
    appstreamcli validate --no-net "$metainfo"
else
    echo "warning: appstreamcli not installed: not validating the AppStream metadata" >&2
fi

# .deb: Debian, Ubuntu, Mint, Pop!_OS and others. Installs the udev rule and applies it.
deb="$stage/deb"
mkdir -p "$deb/DEBIAN"
cp -a "$usr" "$deb/"
cat > "$deb/DEBIAN/control" <<EOF
Package: keyless
Version: $version
Architecture: amd64
Maintainer: Keyless <https://github.com/mrlahmar/keyless/issues>
Depends: libc6 (>= 2.36), libfontconfig1, udev
Section: utils
Priority: optional
Homepage: https://github.com/mrlahmar/keyless
Description: Pause the keyboard while the mouse keeps working
 Keyless pauses the physical keyboard so you can clean it, rest your hands on it
 or keep a cat off it. The mouse keeps working.
EOF
cat > "$deb/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
# Give the user at the screen access to keyboards and /dev/uinput right away.
if [ "$1" = configure ] && command -v udevadm >/dev/null; then
    udevadm control --reload-rules || true
    udevadm trigger --subsystem-match=input --subsystem-match=misc || true
fi
EOF
cat > "$deb/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ] && command -v udevadm >/dev/null; then
    udevadm control --reload-rules || true
fi
EOF
chmod 755 "$deb/DEBIAN/postinst" "$deb/DEBIAN/postrm"
dpkg-deb --build --root-owner-group "$deb" "$dist/keyless_${version}_amd64.deb"

# AppImage: any distro. It can't install the udev rule; Keyless explains how on first start.
if [ -n "${APPIMAGETOOL:-}" ]; then
    app="$stage/Keyless.AppDir"
    mkdir -p "$app"
    cp -a "$usr" "$app/"
    rm -rf "$app/usr/lib/udev"
    install -Dm644 packaging/linux/70-keyless.rules -t "$app/usr/share/keyless/"
    ln -s usr/lib/keyless/keyless "$app/AppRun"
    cp packaging/linux/keyless.desktop assets/keyless.svg "$app/"
    ln -s keyless.svg "$app/.DirIcon"
    ARCH=x86_64 "$APPIMAGETOOL" --no-appstream "$app" "$dist/Keyless-$version-x86_64.AppImage"
else
    echo "warning: APPIMAGETOOL not set: skipping the AppImage" >&2
fi

(cd "$dist" && sha256sum keyless_*.deb Keyless-*.AppImage 2>/dev/null > SHA256SUMS-linux.txt || true)
ls -l "$dist"
