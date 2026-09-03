#!/usr/bin/env bash
# Proves reusable QML Process fallbacks are cleared before every new command.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

D="$FIXTURE_ROOT/flea-process-output-$$"
cleanup() { sandbox_remove "$D"; }
trap cleanup EXIT
sandbox_make "$D"
mkdir -p "$D/bin" "$D/home/.config" "$D/config"

for file in NetworkMounts.qml DeviceMounts.qml Taildrop.qml; do
  ln -s "$PWD/ui/$file" "$D/config/$file"
done
ln -s "$PWD/ui/js" "$D/config/js"
ln -s "$PWD/tests/process-output.qml" "$D/config/shell.qml"

cat > "$D/bin/gio" <<'EOS'
#!/bin/sh
[ "$1 $2" = "mount -l" ] && exit 0
sleep 3
EOS
cat > "$D/bin/lsblk" <<'EOS'
#!/bin/sh
printf '%s\n' '{"blockdevices":[]}'
EOS
cat > "$D/bin/tailscale" <<'EOS'
#!/bin/sh
sleep 3
EOS
chmod +x "$D/bin/"*

out=$(env HOME="$D/home" PATH="$D/bin:/usr/bin:/bin" QT_FORCE_STDERR_LOGGING=1 \
  timeout 7 qs -p "$D/config" 2>&1)
count=$(grep -c 'PROCESS_OUTPUT fresh=yes' <<< "$out")
if [ "$count" -ne 1 ]; then
  printf 'FAIL reusable Process output crossed command boundaries\n%s\n' "$out"
  exit 1
fi

printf 'process-output: all fallbacks start fresh\n'
