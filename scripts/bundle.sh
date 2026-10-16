#!/bin/zsh
set -eu
cd "${0:A:h:h}"
export MACOSX_DEPLOYMENT_TARGET=14.0
cargo build --release --locked
bundle="$PWD/dist/Sysly.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp target/release/sysly "$bundle/Contents/MacOS/Sysly"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Sysly</string>
<key>CFBundleDisplayName</key><string>Sysly</string>
<key>CFBundleExecutable</key><string>Sysly</string>
<key>CFBundleIdentifier</key><string>dev.sysly.monitor</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>CFBundleIconFile</key><string>Sysly.icns</string>
</dict></plist>
PLIST
swift scripts/icon.swift "$PWD/dist"
iconutil -c icns dist/Sysly.iconset -o "$bundle/Contents/Resources/Sysly.icns"
xattr -cr "$bundle"
codesign --force --deep --sign - "$bundle"
ditto -c -k --keepParent --norsrc --noextattr --noqtn "$bundle" "$PWD/dist/Sysly.zip"
print "$bundle"
