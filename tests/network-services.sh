#!/usr/bin/env bash
# Drives the real Quickshell network services with argv-logging stubs and no window.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

D="$FIXTURE_ROOT/flea-network-services-$$"
fail=0
cleanup() { sandbox_remove "$D"; }
trap cleanup EXIT
sandbox_make "$D"
mkdir -p "$D/bin" "$D/home/.config" "$D/config"
ln -s "$PWD/tests/network-services.qml" "$D/config/shell.qml"
ln -s "$PWD/ui/NetworkDiscovery.qml" "$D/config/NetworkDiscovery.qml"
ln -s "$PWD/ui/NetworkHistory.qml" "$D/config/NetworkHistory.qml"
ln -s "$PWD/ui/NetworkActions.qml" "$D/config/NetworkActions.qml"
ln -s "$PWD/ui/NetworkMounts.qml" "$D/config/NetworkMounts.qml"
ln -s "$PWD/ui/js" "$D/config/js"

check() {
  local label="$1" expected="$2" actual="$3"
  if [[ "$expected" != "$actual" ]]; then
    printf 'FAIL %s\n  expected: %s\n  actual:   %s\n' "$label" "$expected" "$actual"
    fail=1
  else
    printf 'ok   %s\n' "$label"
  fi
}

cat > "$D/bin/tailscale" <<'EOS'
#!/bin/sh
case "${FLEA_TS_CASE:-ready}" in
  ready) printf '%s\n' '{"BackendState":"Running","Self":{"UserID":7},"Peer":{"p":{"HostName":"tailbox","Online":true,"TaildropTarget":1,"UserID":7,"TailscaleIPs":["100.64.0.8"]}}}' ;;
  missing) echo 'command not found' >&2; exit 127 ;;
  stopped) echo 'failed to connect to local tailscaled; not running' >&2; exit 1 ;;
  loggedout) printf '%s\n' '{"BackendState":"NeedsLogin","Peer":{}}' ;;
  nopeers) printf '%s\n' '{"BackendState":"Running","Peer":{}}' ;;
  malformed) printf '%s\n' 'not json' ;;
  timeout) sleep 10 ;;
esac
EOS
cat > "$D/bin/avahi-browse" <<'EOS'
#!/bin/sh
case "${FLEA_LAN_CASE:-ready}" in
  ready) printf '%s\n' '=;eth0;IPv4;LAN Box;_ssh._tcp;local;lanbox.local;192.168.1.8;22;"mac=aa:bb:cc:dd:ee:ff"' ;;
  empty) : ;;
  malformed) printf '%s\n' 'not avahi output' ;;
  failed) exit 1 ;;
  timeout) sleep 10 ;;
esac
EOS
cat > "$D/bin/wl-copy" <<'EOS'
#!/bin/sh
printf 'copy:%s:<%s>\n' "$#" "$1" >> "$FLEA_STUB_LOG"
[ "${FLEA_ACTION_FAIL:-0}" = 1 ] && exit 1
exit 0
EOS
cat > "$D/bin/flea-wake" <<'EOS'
#!/bin/sh
printf 'wake:%s:<%s>:<%s>\n' "$#" "$1" "$2" >> "$FLEA_STUB_LOG"
[ "${FLEA_ACTION_FAIL:-0}" = 1 ] && exit 1
exit 0
EOS
cat > "$D/bin/gio" <<'EOS'
#!/bin/sh
if [ "$1" = list ]; then sleep 20; exit 0; fi
if [ "$1 $2" = 'mount -l' ]; then exit 0; fi
exit 1
EOS
chmod +x "$D/bin/"*

run_service() {
  env HOME="$D/home" PATH="$D/bin:/usr/bin:/bin" FLEA_STUB_LOG="$D/actions.log" \
    FLEA_BIN="$D/bin/flea-wake" QT_FORCE_STDERR_LOGGING=1 timeout 12 \
    qs -p "$D/config" 2>&1
}

out=$(FLEA_TS_CASE=ready FLEA_LAN_CASE=ready FLEA_SERVICE_MODE=discover run_service)
check "ready discovery crosses both real Process boundaries" "1" "$(grep -c 'DISCOVERY ready ready peers=1 lan=1' <<< "$out")"

for spec in \
  'missing missing Tailscale is unavailable' \
  'stopped daemon-stopped Tailscale is stopped' \
  'loggedout logged-out Tailscale is logged out' \
  'nopeers no-peers No machines were found' \
  'malformed failed unreadable status'; do
  read -r scenario state words <<< "$spec"
  out=$(FLEA_TS_CASE="$scenario" FLEA_LAN_CASE=empty FLEA_SERVICE_MODE=discover run_service)
  check "Tailscale $scenario has its precise state" "1" "$(grep -c "DISCOVERY $state empty" <<< "$out")"
  check "Tailscale $scenario has recovery/status guidance" "1" "$(grep -c "$words" <<< "$out")"
done

for scenario in empty malformed failed; do
  out=$(FLEA_TS_CASE=ready FLEA_LAN_CASE="$scenario" FLEA_SERVICE_MODE=discover run_service)
  want="$scenario"
  [[ "$scenario" = malformed ]] && want=empty
  [[ "$scenario" = failed ]] && want=unavailable
  check "LAN $scenario remains optional" "1" "$(grep -c "DISCOVERY ready $want peers=1 lan=0" <<< "$out")"
done

out=$(FLEA_TS_CASE=timeout FLEA_LAN_CASE=timeout FLEA_SERVICE_MODE=discover run_service)
check "wedged discovery processes are bounded" "1" "$(grep -c 'DISCOVERY failed failed peers=0 lan=0' <<< "$out")"
out=$(FLEA_SERVICE_MODE=list-timeout run_service)
check "a wedged share listing is bounded and visible" "1" "$(grep -c 'MOUNTS error Listing shares timed out' <<< "$out")"

: > "$D/actions.log"
out=$(FLEA_SERVICE_MODE=copy run_service)
check "copy reports success only after the command exits" "1" "$(grep -c 'ACTION ok Network address copied' <<< "$out")"
check "copy preserves a hostile address as one argv item" 'copy:1:<host; touch should-not-exist>' "$(cat "$D/actions.log")"
[[ ! -e should-not-exist ]] || { printf 'FAIL hostile clipboard text executed as shell\n'; fail=1; }

: > "$D/actions.log"
out=$(FLEA_SERVICE_MODE=wake run_service)
check "Wake reports success only after the command exits" "1" "$(grep -c 'ACTION ok Wake-on-LAN packet sent' <<< "$out")"
check "Wake uses an argv-direct internal mode" 'wake:2:<--wake>:<aa:bb:cc:dd:ee:ff>' "$(cat "$D/actions.log")"

: > "$D/actions.log"
out=$(FLEA_ACTION_FAIL=1 FLEA_SERVICE_MODE=copy run_service)
check "clipboard command failure is visible" "1" "$(grep -c 'ACTION error The network address could not be copied' <<< "$out")"
out=$(FLEA_ACTION_FAIL=1 FLEA_SERVICE_MODE=wake run_service)
check "Wake command failure is visible" "1" "$(grep -c 'ACTION error Wake-on-LAN could not send' <<< "$out")"

out=$(FLEA_SERVICE_MODE=history run_service)
check "successful-open history and Wake profile cross FileView" "1" "$(grep -c 'HISTORY recent=1 profiles=1' <<< "$out")"
check "history strips URI passwords and queries" "1" "$(grep -c 'sftp://pi@box/home' "$D/home/.config/flea-network-recents.json")"
check "history stores no secret material" "0" "$(grep -Ec 'secret|token=bad' "$D/home/.config/flea-network-recents.json")"

[[ "$fail" -eq 0 ]] && printf 'network-services: all checks passed\n'
exit "$fail"
