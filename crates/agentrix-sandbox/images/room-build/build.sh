#!/bin/sh
# room-build (S1-02; design §3.1; R6.1, R12.2): runs an old theme's build hook in a sealed room
# with no network, under the room's CPU, memory, pids and time caps. A hostile hook (gauntlet
# attack 9: a fork bomb and an 8 GB allocation) is killed by those caps; the runner reports
# `killed` and `destroyed`, and the Move retries without it.
#
# Contract (move lane, agentrix-move pipeline.rs, Build job):
#   /in/job.json   {step: "legacy-theme"|"site", theme}
#   /in/<hex>      the old site's wp-content.tar.gz (one input)
#   /out/build.log the hook's output, cut at 64 KB
#   stdout         {"event":"note","text":...}
set -u
J=/in/job.json
step=$(jq -r '.step // "site"' "$J")
theme=$(jq -r '.theme // ""' "$J")
note() { printf '{"event":"note","text":"%s"}\n' "$1"; }

case "$theme" in
  ""|*/*|*..*) note "no usable theme name"; exit 0 ;;
esac

if [ "$step" != "legacy-theme" ]; then
  note "nothing to build for step $step"
  exit 0
fi

input=$(ls /in | grep -v '^job.json$' | head -n 1)
[ -n "$input" ] || { note "no wp-content input"; exit 2; }
mkdir -p /tmp/wp
tar -xzf "/in/$input" -C /tmp/wp 2>/dev/null || { note "wp-content did not unpack"; exit 2; }
hook=""
for candidate in "/tmp/wp/themes/$theme/build.sh" "/tmp/wp/wp-content/themes/$theme/build.sh"; do
  [ -f "$candidate" ] && hook="$candidate" && break
done
[ -n "$hook" ] || { note "theme $theme has no build hook"; exit 0; }

note "running $theme's build hook"
cd "$(dirname "$hook")" && sh "$hook" 2>&1 | head -c 65536 >/out/build.log
status=$?
note "build hook exited $status"
exit "$status"
