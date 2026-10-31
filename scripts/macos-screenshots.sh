#!/usr/bin/env bash
# Takes the screenshots of macOS blocking BloogBase on its first start and of
# "Open Anyway" in System Settings (docs/screenshots.md). Run by
# .github/workflows/macos-screenshots.yml on a GitHub macOS runner; it changes
# system settings, so don't run it on your own Mac.
#
#   scripts/macos-screenshots.sh <BloogBase.app> <output folder> <lang>...
#
# For every language (en, de) it installs a fresh copy of the app in
# /Applications marked as downloaded (quarantine), starts it and takes:
#   blocked.<lang>.png      the "BloogBase Not Opened" dialog
#   settings.<lang>.png     Privacy & Security with "Open Anyway"
#   confirm.<lang>.png      what comes after clicking it (best effort)
#   screen-*.<lang>.png     the whole screen at each step, to check or crop
# and logs the texts and buttons it finds, for the README's wording.

set -uo pipefail

app_src=$1
out=$2
shift 2
mkdir -p "$out"
work=$(mktemp -d)

# Lists the windows of a process as "id width height layer", largest first.
# Window ids and sizes need no screen recording permission.
cat > "$work/windows.swift" <<'SWIFT'
import CoreGraphics
import Foundation

let pid = Int(CommandLine.arguments[1]) ?? -1
let info = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
let windows = (info as? [[String: Any]] ?? []).compactMap { w -> (Int, Int, Int, Int)? in
  guard (w[kCGWindowOwnerPID as String] as? Int) == pid,
        let id = w[kCGWindowNumber as String] as? Int,
        let bounds = w[kCGWindowBounds as String] as? [String: Any],
        let width = (bounds["Width"] as? NSNumber)?.intValue,
        let height = (bounds["Height"] as? NSNumber)?.intValue,
        width > 100, height > 50 else { return nil }
  return (id, width, height, w[kCGWindowLayer as String] as? Int ?? 0)
}
for w in windows.sorted(by: { $0.1 * $0.2 > $1.1 * $1.2 }) {
  print(w.0, w.1, w.2, w.3)
}
SWIFT
swiftc -O -o "$work/windows" "$work/windows.swift" || exit 1

# The id of the largest window of the process named $1, waiting up to $2 s.
window_of() {
  local name=$1 tries=${2:-30} pid id
  for _ in $(seq 1 "$tries"); do
    pid=$(pgrep -x "$name" | head -n 1)
    if [ -n "$pid" ]; then
      id=$(limit 5 "$work/windows" "$pid" | head -n 1 | cut -d ' ' -f 1)
      if [ -n "$id" ]; then
        echo "$id"
        return 0
      fi
    fi
    sleep 1
  done
  return 1
}

# Every on-screen window, for the log when a step doesn't find its window.
list_windows() {
  local pid found
  for pid in $(ps -axo pid=); do
    found=$(limit 5 "$work/windows" "$pid")
    [ -n "$found" ] && echo "  $(ps -o comm= -p "$pid"): $found"
  done
}

# Prints a timestamped line, to see in the log where a step hangs.
step() {
  echo "[$(date +%H:%M:%S)] $*"
}

# Runs a command and kills it after $1 seconds. Asking System Events or
# capturing the screen can wait forever on a permission prompt nobody
# answers; both hung the first runs. Keeps the caller's standard input
# (osascript reads its script from a heredoc).
limit() {
  local secs=$1 pid watchdog status
  shift
  "$@" <&0 &
  pid=$!
  # Not on the caller's output: in $(...), its sleep would keep the pipe
  # open and every call would last the full $secs.
  (sleep "$secs" && kill -9 "$pid") >/dev/null 2>&1 &
  watchdog=$!
  wait "$pid"
  status=$?
  kill "$watchdog" 2>/dev/null
  wait "$watchdog" 2>/dev/null
  [ "$status" -eq 137 ] && echo "gave up after $secs s: $1 (permission prompt?)" >&2
  return "$status"
}

# osascript for the log only: never fails the run.
osa() {
  limit 30 osascript "$@"
  return 0
}

shot() {
  local name=$1 window=$2
  step "shot $name"
  limit 20 screencapture -x "$out/screen-$name.png" || echo "::warning::Full-screen capture failed ($name)"
  if [ -n "$window" ]; then
    limit 20 screencapture -x -o -l "$window" "$out/$name.png" || echo "::warning::Window capture failed ($name)"
  fi
}

# Grants screen recording, accessibility and automation in the system TCC
# database, as GitHub's images allow at runtime; without screen recording,
# screencapture waits forever on macOS 15. Granted to the shell, the tools
# and every process above this script, as macOS asks the "responsible" one.
grant_permissions() {
  local db="/Library/Application Support/com.apple.TCC/TCC.db" now clients pid exe client service
  echo "macOS $(sw_vers -productVersion), $(csrutil status 2>&1)"
  now=$(date +%s)
  clients="/bin/bash /bin/sh /usr/sbin/screencapture /usr/bin/osascript"
  pid=$$
  while [ "$pid" -gt 1 ]; do
    exe=$(ps -o comm= -p "$pid" 2>/dev/null)
    case $exe in /*) clients="$clients $exe" ;; esac
    pid=$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')
    [ -n "$pid" ] || break
  done
  for client in $clients; do
    for service in kTCCServiceScreenCapture kTCCServiceAccessibility kTCCServicePostEvent; do
      sudo sqlite3 "$db" "INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, flags, last_modified) VALUES ('$service', '$client', 1, 2, 4, 1, 0, $now);" 2>&1 |
        sed "s|^|  tcc $service $client: |"
    done
    sudo sqlite3 "$db" "INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, indirect_object_identifier, flags, last_modified) VALUES ('kTCCServiceAppleEvents', '$client', 1, 2, 4, 1, 'com.apple.systemevents', 0, $now);" 2>&1 |
      sed "s|^|  tcc AppleEvents $client: |"
  done
  echo "Granted to: $clients"
  sudo sqlite3 "$db" "SELECT service, client, auth_value FROM access WHERE service = 'kTCCServiceScreenCapture';" 2>&1 |
    sed 's/^/  screen capture: /'
  sudo killall tccd 2>/dev/null
  killall tccd 2>/dev/null
  sleep 1
}

grant_permissions

# Gatekeeper is off on GitHub's runners; the shots need it on.
echo "Gatekeeper before: $(spctl --status 2>&1)"
sudo spctl --global-enable 2>/dev/null || sudo spctl --master-enable
echo "Gatekeeper now: $(spctl --status 2>&1)"

failed=0
for lang in "$@"; do
  echo "::group::Screenshots in $lang"
  case $lang in
    de) locale=de_DE ;;
    *) locale=en_US ;;
  esac
  defaults write -g AppleLanguages -array "$lang"
  defaults write -g AppleLocale "$locale"
  killall "System Settings" CoreServicesUIAgent 2>/dev/null
  sleep 2

  # A fresh copy each time, as if just downloaded with Safari, so an earlier
  # language's attempt doesn't count.
  sudo rm -rf /Applications/BloogBase.app
  ditto "$app_src" /Applications/BloogBase.app
  xattr -w com.apple.quarantine "0083;$(printf %x "$(date +%s)");Safari;$(uuidgen)" \
    /Applications/BloogBase.app
  spctl --assess --type execute -vv /Applications/BloogBase.app 2>&1 | sed 's/^/  spctl: /'

  step "open the app"
  # In the background: when Gatekeeper blocks the app, `open` waits for an
  # answer to the dialog, which hung the earlier runs.
  open /Applications/BloogBase.app &
  step "wait for the dialog"
  if dialog=$(window_of CoreServicesUIAgent 30); then
    sleep 1
    shot "blocked.$lang" "$dialog"
    osa -e "tell application \"System Events\" to get entire contents of window 1 of (first process whose unix id is $(pgrep -x CoreServicesUIAgent | head -n 1))" 2>&1 |
      sed 's/^/  dialog: /'
  else
    echo "::error::macOS didn't block BloogBase ($lang). Windows on screen:"
    list_windows
    shot "blocked.$lang" ""
    failed=1
  fi
  # Answer the dialog as a player would: System Settings only offers Open
  # Anyway for an app whose block was acknowledged.
  if pid=$(pgrep -x CoreServicesUIAgent | head -n 1); then
    osa - "$pid" <<'APPLESCRIPT' 2>&1 | sed 's/^/  dialog: /'
on run argv
  tell application "System Events"
    tell (first process whose unix id is ((item 1 of argv) as integer))
      repeat with e in (entire contents of window 1)
        try
          if (role of e) is "AXButton" and (name of e) is in {"Done", "Fertig", "OK"} then
            click e
            return "clicked " & (name of e)
          end if
        end try
      end repeat
      return "no Done button found"
    end tell
  end tell
end run
APPLESCRIPT
  fi
  sleep 2
  killall CoreServicesUIAgent 2>/dev/null
  sleep 1

  step "open Privacy & Security"
  open "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension"
  if settings=$(window_of "System Settings" 30); then
    sleep 3
    # Scroll down to the block, which sits in the Security section, and log
    # its text and button. Needs accessibility access for osascript; without
    # it (see `osa`) the shot shows the top of the pane.
    pid=$(pgrep -x "System Settings" | head -n 1)
    osa - "$pid" <<'APPLESCRIPT' 2>&1 | sed 's/^/  settings: /'
on run argv
  tell application "System Events"
    tell (first process whose unix id is ((item 1 of argv) as integer))
      set frontmost to true
      set found to false
      repeat with e in (entire contents of window 1)
        repeat with attr in {"value", "name", "description"}
          try
            if attr as text is "value" then
              set v to (value of e) as text
            else if attr as text is "name" then
              set v to (name of e) as text
            else
              set v to (description of e) as text
            end if
            if v contains "BloogBase" and not found then
              log "text (" & attr & "): " & v
              perform action "AXScrollToVisible" of e
              set found to true
            end if
          end try
        end repeat
        try
          if (role of e) is "AXButton" and (name of e) is not missing value then
            log "button: " & (name of e)
          end if
        end try
      end repeat
      if not found then
        log "no text with BloogBase found; scrolling to the end"
        repeat with e in (entire contents of window 1)
          try
            if (role of e) is "AXScrollBar" and (orientation of e) is "AXVerticalOrientation" then
              set value of e to 1.0
              log "scrolled a scroll bar to the end"
            end if
          end try
        end repeat
      end if
    end tell
  end tell
end run
APPLESCRIPT
    sleep 1
    shot "settings.$lang" "$settings"

    # Click the button next to the block (its label depends on the language)
    # and take whatever asks next.
    osa - "$pid" <<'APPLESCRIPT' 2>&1 | sed 's/^/  click: /'
on run argv
tell application "System Events"
  tell (first process whose unix id is ((item 1 of argv) as integer))
    repeat with e in (entire contents of window 1)
      try
        if (role of e) is "AXButton" and (name of e) is in {"Open Anyway", "Dennoch öffnen", "Trotzdem öffnen"} then
          click e
          return "clicked " & (name of e)
        end if
      end try
    end repeat
    return "no Open Anyway button found"
  end tell
end tell
end run
APPLESCRIPT
    sleep 4
    echo "  windows after the click:"
    list_windows
    confirm=$(window_of SecurityAgent 3 || window_of CoreServicesUIAgent 3 || true)
    shot "confirm.$lang" "$confirm"
  else
    echo "::error::System Settings didn't open ($lang)."
    shot "settings.$lang" ""
    failed=1
  fi

  killall "System Settings" CoreServicesUIAgent BloogBase 2>/dev/null
  sudo killall SecurityAgent 2>/dev/null
  echo "::endgroup::"
done

exit "$failed"
