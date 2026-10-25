#!/usr/bin/env bash
# Pull-based gym updates. A systemd timer runs this every few minutes on the gym host: when the gym's
# files on main changed since the last deploy, it checks them out and runs deploy.sh. Nothing
# outside the host can trigger it, which keeps a public repo's pull requests away from the box.
set -euo pipefail

CHECKOUT=${GYM_CHECKOUT:-/opt/rustsheet-grid}
STATE_DIR=${GYM_HOST_STATE_DIR:-/var/lib/rustsheet-gym}
DEPLOYED="$STATE_DIR/deployed-tree"   # gym tree hash of the running gym
FAILED="$STATE_DIR/failed-tree"       # last tree hash that failed, so it is not retried every tick

cd "$CHECKOUT"
git fetch --quiet origin main
# What the gym is built from: the gym directory plus the lockfile and workspace manifests.
tree=$(git rev-parse origin/main:gym origin/main:pnpm-lock.yaml origin/main:package.json | sha256sum | cut -c1-40)

[ "$(cat "$DEPLOYED" 2>/dev/null)" = "$tree" ] && exit 0
[ "$(cat "$FAILED" 2>/dev/null)" = "$tree" ] && exit 0

git checkout --quiet --detach origin/main
echo "gym files changed on main ($(git rev-parse --short HEAD)); deploying"
if gym/deploy/deploy.sh; then
  echo "$tree" > "$DEPLOYED"
  rm -f "$FAILED"
else
  echo "$tree" > "$FAILED"
  exit 1
fi
