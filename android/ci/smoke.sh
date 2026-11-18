#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Ferrum for Android's smoke test, run on an emulator by GitHub Actions:
# installs the app, goes through the main screens and saves a screenshot of
# each, and fails if the app crashed.
#   android/ci/smoke.sh <app.apk> <screenshot folder>
set -uo pipefail
APK=$1
OUT=$2
PKG=io.github.p1e2r.ferrum.debug
mkdir -p "$OUT"

shot() { sleep "${2:-2}"; adb exec-out screencap -p > "$OUT/$1.png"; echo "shot $1"; }

# Taps the element with this test tag (resource-id) or text.
tap() {
  adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1
  adb shell cat /sdcard/ui.xml > "$OUT/ui.xml"
  python3 - "$1" "$OUT/ui.xml" <<'PY' | { read -r x y && adb shell input tap "$x" "$y" && echo "tapped $1 at $x,$y" || echo "not found: $1"; }
import re, sys, xml.etree.ElementTree as ET
want, path = sys.argv[1], sys.argv[2]
for n in ET.parse(path).iter("node"):
    if want in (n.get("resource-id"), n.get("text"), n.get("content-desc")):
        a, b, c, d = map(int, re.findall(r"\d+", n.get("bounds")))
        print((a + c) // 2, (b + d) // 2)
        break
PY
}

adb logcat -c
adb install -r "$APK" || exit 1
adb shell am start -W -n "$PKG/io.github.p1e2r.ferrum.MainActivity"
shot 01-start 6

tap address
adb shell input text "1024/8"
shot 02-calculator

adb shell input keycombination 113 29   # Ctrl+A
adb shell input keyevent 67             # delete
adb shell input text "example.com"
shot 03-typing 1
adb shell input keyevent 66             # Enter
shot 04-page 8

tap menu
shot 05-menu
adb shell input keyevent 4

adb shell am start -a android.intent.action.VIEW -d "https://en.wikipedia.org/wiki/Iron" -n "$PKG/io.github.p1e2r.ferrum.MainActivity"
shot 06-link-from-another-app 10

tap tabs
shot 07-tabs
adb shell input keyevent 4

tap menu
sleep 1
tap Settings
shot 08-settings
adb shell input keyevent 4

adb shell cmd uimode night yes
tap tabs
sleep 1
tap "New tab"
shot 09-dark-start 3
adb shell cmd uimode night no

adb logcat -d > "$OUT/logcat.txt"
if grep -q "FATAL EXCEPTION" "$OUT/logcat.txt"; then
  echo "::error::Ferrum crashed:"
  grep -A 30 "FATAL EXCEPTION" "$OUT/logcat.txt"
  exit 1
fi
echo "No crashes"
