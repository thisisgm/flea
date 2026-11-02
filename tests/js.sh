#!/bin/bash
# Runs the pure JavaScript suites under qml6, with no Quickshell and no window.
set -u
cd "$(dirname "$0")/.." || exit 1

if [ "$#" -gt 1 ]; then
  echo "js.sh: expected at most one suite name"
  exit 2
fi
if [ "$#" -eq 1 ]; then
  case "$1" in
    "") echo "js.sh: suite name cannot be empty"; exit 2 ;;
    --) echo "js.sh: -- is not a suite name"; exit 2 ;;
  esac
fi

if ! command -v qml6 >/dev/null; then
  echo "js.sh: qml6 is not installed, cannot run the pure JavaScript suites"
  exit 1
fi

# Read the runtime used below, so the sequence suite checks the shape required by that Qt version.
if ! qml_version=$(QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic qml6 --version); then
  echo "FAIL: qml6 --version exited non-zero, so the Qt version for tests/js/sequences.js is unknown" >&2
  exit 1
fi
# Sample input: 'Qml Runtime 6.12.0'. The version is the last word.
qt_version=${qml_version##* }

run() {
  local suite_args=()
  local out code
  [ -n "${2:-}" ] && suite_args=("$2")
  # tests/js/themes.js reads the installed colors.toml files, which qml6 refuses without this.
  out=$(TZ="$1" QML_XHR_ALLOW_FILE_READ=1 QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 timeout 120 qml6 tests/js/harness.qml -- "${suite_args[@]}" "--qt-version=$qt_version" 2>&1)
  code=$?
  echo "$out"
  if [ "$code" = 124 ]; then
    echo "js.sh: the harness did not finish inside 120s, which a load error does"
    exit 1
  fi
  # qml6 exits 0 on a ReferenceError inside an imported library, so the tally proves execution.
  if ! printf '%s' "$out" | grep -q "checks, "; then
    echo "js.sh: the harness reported no tally at all, so nothing ran"
    exit 1
  fi
  printf '%s' "$out" | grep -qE "checks, 0 failed" || exit 1
  if [ -z "${2:-}" ] || [ "$2" = devices ]; then
    # Sample input: "qml: power-off warning begin: Device listing for /dev/sda could not be parsed (JSON.parse: Parse error); mounted rail volumes were queued instead.", then "qml: Device listing for /dev/sda could not be parsed (JSON.parse: Parse error); mounted rail volumes were queued instead.", then "qml: power-off warning end".
    local active=0 expected="" warnings=0 bad=0 fallbacks=0 valid=0 line
    while IFS= read -r line; do
      case "$line" in
        "qml: power-off warning begin: "*)
          [ "$active" -eq 0 ] || bad=1
          expected=${line#"qml: power-off warning begin: "}
          active=1
          warnings=0
          ;;
        "qml: power-off warning end")
          [ "$active" -eq 1 ] || bad=1
          if [ -n "$expected" ]; then
            [ "$warnings" -eq 1 ] || bad=1
            fallbacks=$((fallbacks + 1))
          else
            [ "$warnings" -eq 0 ] || bad=1
            valid=$((valid + 1))
          fi
          active=0
          ;;
        *)
          if [ "$active" -eq 1 ]; then
            warnings=$((warnings + 1))
            [ "$line" = "qml: $expected" ] || bad=1
          fi
          ;;
      esac
    done <<< "$out"
    if [ "$active" -ne 0 ] || [ "$bad" -ne 0 ] || [ "$fallbacks" -eq 0 ] || [ "$valid" -eq 0 ]; then
      echo "FAIL power-off warning: expected one exact warning per fallback and none for valid listings"
      exit 1
    fi
    printf 'ok   power-off warnings: %d fallbacks, %d valid listings\n' "$fallbacks" "$valid"
  fi
  [ "$code" = 0 ] || exit "$code"
}

# Caller controls the standard battery's timezone; unattended runs default to positive-offset Tokyo.
normal_timezone=${TZ-Asia/Tokyo}
if [ "$#" -eq 1 ]; then
  suite_timezone=$normal_timezone
  case "$1" in
    dst) suite_timezone=America/New_York ;;
    edmonton) suite_timezone=America/Edmonton ;;
  esac
  run "$suite_timezone" "$1"
  exit 0
fi

run "$normal_timezone"
# Reporter cases run under Edmonton, while DST-only fixtures run under New York.
run America/Edmonton edmonton
run America/New_York dst
