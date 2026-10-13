#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
profile=${1:-release}
case "$profile" in
    debug) release_flag= ;;
    release) release_flag=--release ;;
    *) echo "Usage: $0 [debug|release]" >&2; exit 2 ;;
esac

if [ "$(uname -s)" != Darwin ]; then
    echo "This bundle script requires macOS." >&2
    exit 1
fi

cargo_command=${CARGO:-cargo}
"$cargo_command" build --locked --manifest-path "$root/Cargo.toml" $release_flag
target=${CARGO_TARGET_DIR:-"$root/target"}
bundle="$target/Ryne.app"
icon_work=$(mktemp -d "$target/.ryne-icon.XXXXXX")
trap 'rm -rf "$icon_work"' 0
trap 'exit 1' HUP INT TERM
iconset="$icon_work/Ryne.iconset"
"$cargo_command" run --locked --manifest-path "$root/Cargo.toml" \
    --example app-icon $release_flag -- "$root/assets/logo.svg" "$iconset"
iconutil --convert icns --output "$icon_work/Ryne.icns" "$iconset"

mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$target/$profile/ryne" "$bundle/Contents/MacOS/Ryne.next"
mv -f "$bundle/Contents/MacOS/Ryne.next" "$bundle/Contents/MacOS/Ryne"
cp "$icon_work/Ryne.icns" "$bundle/Contents/Resources/Ryne.icns.next"
mv -f "$bundle/Contents/Resources/Ryne.icns.next" "$bundle/Contents/Resources/Ryne.icns"
# STIX Two Math is embedded in the executable; ship its license alongside it.
mkdir -p "$bundle/Contents/Resources/stix-two-math"
for notice in OFL.txt FONTLOG.txt; do
    cp "$root/assets/fonts/stix-two-math/$notice" "$bundle/Contents/Resources/stix-two-math/$notice.next"
    mv -f "$bundle/Contents/Resources/stix-two-math/$notice.next" "$bundle/Contents/Resources/stix-two-math/$notice"
done
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Ryne</string>
<key>CFBundleDisplayName</key><string>Ryne</string>
<key>CFBundleIdentifier</key><string>dev.ryne.editor</string>
<key>CFBundleExecutable</key><string>Ryne</string>
<key>CFBundleIconFile</key><string>Ryne.icns</string>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleLocalizations</key><array>
<string>zh-Hans</string><string>zh-Hant</string><string>en</string>
<string>ja</string><string>ko</string><string>fr</string><string>es</string>
</array>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$bundle"
touch "$bundle"
# Refresh only this bundle's registered metadata after an in-place rebuild.
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$lsregister" -f "$bundle"
echo "$bundle"
