#!/bin/bash
# make-claude-instance.sh
# Create a color-coded, isolated Claude Desktop launcher on macOS.
#
# Usage:
#   ./make-claude-instance.sh <instance> <DisplayName> [hueShift] [satGain]
#
#   instance     Profile name. Use "default" for Claude's built-in profile
#                (personal). Any other name => isolated profile under
#                ~/.claude-instances/<instance>.
#   DisplayName  Shows in the Dock/Launchpad as "Claude <DisplayName>".
#   hueShift     Optional. Icon hue rotation, 0-255 (==0-360deg). e.g. 135=blue,
#                -7=red-orange, 80=green, 170=purple. Default 0 (stock orange).
#   satGain      Optional. Saturation multiplier (default 1.0).
#
# Icon recoloring requires Python 3 + Pillow (pip install Pillow). Without them
# the launcher still works, just with the stock orange icon.
set -e

INSTANCE="$1"
DISPLAY_NAME="$2"
HUE_SHIFT="${3:-0}"
SAT_GAIN="${4:-1.0}"

CLAUDE_APP="/Applications/Claude.app"
INSTANCES_BASE="$HOME/.claude-instances"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ -z "$INSTANCE" ] || [ -z "$DISPLAY_NAME" ]; then
    echo "Usage: $0 <instance> <DisplayName> [hueShift] [satGain]"; exit 1
fi
if [ ! -d "$CLAUDE_APP" ]; then
    echo "❌ Claude Desktop not found at $CLAUDE_APP — install from https://claude.ai/download"; exit 1
fi

WRAPPER="/Applications/Claude $DISPLAY_NAME.app"
if [ -d "$WRAPPER" ]; then
    read -p "⚠️  '$WRAPPER' exists. Overwrite? (y/N): " ow
    [ "$ow" = "y" ] || [ "$ow" = "Y" ] || { echo "Cancelled."; exit 0; }
    rm -rf "$WRAPPER"
fi

mkdir -p "$WRAPPER/Contents/MacOS" "$WRAPPER/Contents/Resources"
[ "$INSTANCE" != "default" ] && mkdir -p "$INSTANCES_BASE/$INSTANCE"

# ---- Info.plist -------------------------------------------------------------
cat > "$WRAPPER/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>claude-launcher</string>
    <key>CFBundleIdentifier</key><string>com.anthropic.claude.$INSTANCE</string>
    <key>CFBundleName</key><string>Claude $DISPLAY_NAME</string>
    <key>CFBundleDisplayName</key><string>Claude $DISPLAY_NAME</string>
    <key>CFBundleVersion</key><string>1.0</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>CFBundleIconFile</key><string>claude-icon</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

# ---- config.sh --------------------------------------------------------------
echo "INSTANCE_NAME=$INSTANCE" > "$WRAPPER/Contents/MacOS/config.sh"

# ---- smart launcher (self-contained) ----------------------------------------
cat > "$WRAPPER/Contents/MacOS/claude-launcher" <<'LAUNCHER'
#!/bin/bash
source "$(dirname "$0")/config.sh"
CLAUDE_APP="/Applications/Claude.app"
INSTANCES_BASE="$HOME/.claude-instances"
if [ "$INSTANCE_NAME" = "default" ]; then
    PID=$(ps -axo pid,command | grep "[C]laude.app/Contents/MacOS/Claude" | grep -v -- "--user-data-dir" | awk '{print $1}' | head -1)
    LAUNCH=(open -n -a "$CLAUDE_APP")
else
    DIR="$INSTANCES_BASE/$INSTANCE_NAME"
    PID=$(ps -axo pid,command | grep -F -- "MacOS/Claude --user-data-dir=$DIR" | awk '{print $1}' | head -1)
    mkdir -p "$DIR"
    LAUNCH=(open -n -a "$CLAUDE_APP" --args --user-data-dir="$DIR")
fi
if [ -n "$PID" ]; then
    osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $PID) to true" 2>/dev/null
else
    "${LAUNCH[@]}"
fi
LAUNCHER
chmod +x "$WRAPPER/Contents/MacOS/claude-launcher"

# ---- icon (recolor if Pillow available, else copy stock) --------------------
SRC_ICNS=$(find "$CLAUDE_APP" -name "*.icns" -type f 2>/dev/null | head -n 1)
DEST_ICNS="$WRAPPER/Contents/Resources/claude-icon.icns"
RECOLORED=false
if [ -n "$SRC_ICNS" ]; then
    if [ "$HUE_SHIFT" != "0" ] || [ "$SAT_GAIN" != "1.0" ]; then
        if python3 -c "import PIL" 2>/dev/null && [ -f "$SCRIPT_DIR/recolor-icon.py" ]; then
            TMP=$(mktemp -d)
            iconutil -c iconset "$SRC_ICNS" -o "$TMP/icon.iconset"
            python3 "$SCRIPT_DIR/recolor-icon.py" "$TMP/icon.iconset" "$HUE_SHIFT" "$SAT_GAIN" >/dev/null
            iconutil -c icns "$TMP/icon.iconset" -o "$DEST_ICNS"
            rm -rf "$TMP"
            RECOLORED=true
        else
            echo "ℹ️  Skipping recolor (need: python3 + Pillow + recolor-icon.py alongside). Using stock icon."
        fi
    fi
    $RECOLORED || cp "$SRC_ICNS" "$DEST_ICNS"
else
    echo "⚠️  No source icon found; wrapper will use the default macOS icon."
fi

# ---- register & refresh -----------------------------------------------------
LSREG=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$LSREG" -f "$WRAPPER" 2>/dev/null || true
touch "$WRAPPER"
killall Dock 2>/dev/null || true

echo "✅ Created: $WRAPPER"
[ "$INSTANCE" != "default" ] && echo "   Profile: $INSTANCES_BASE/$INSTANCE"
$RECOLORED && echo "   Icon recolored (hue $HUE_SHIFT, sat x$SAT_GAIN)"
echo "   Find 'Claude $DISPLAY_NAME' in Launchpad or drag it to your Dock."
