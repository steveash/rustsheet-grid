#!/usr/bin/env bash
# Deploys the gym on its host: build an image, smoke-test it as a canary beside the running gym,
# and only then replace the running gym. A failed build or canary leaves the running gym alone.
#
# Run by updater.sh (systemd timer) when gym files change on main; safe to run by hand.
#   gym/deploy/deploy.sh            deploy the current checkout
#   gym/deploy/deploy.sh --rollback restart the previous image
set -euo pipefail

ENV_FILE=${GYM_ENV_FILE:-/etc/rustsheet-gym/gym.env}
STATE_DIR=${GYM_HOST_STATE_DIR:-/var/lib/rustsheet-gym}
LIVE_PORT=${GYM_LIVE_PORT:-8787}
CANARY_PORT=${GYM_CANARY_PORT:-8788}
IMAGE=rustsheet-gym
REPO_ROOT=$(cd "$(dirname "$0")/../.." && pwd)

log() { printf '%s %s\n' "$(date -u +%H:%M:%S)" "$*"; }
die() { log "FAILED: $*"; exit 1; }

[ -r "$ENV_FILE" ] || die "missing $ENV_FILE (see gym/deploy/gym.env.example)"
# shellcheck disable=SC1090
TOKEN=$(. "$ENV_FILE" && printf '%s' "$GYM_TOKEN")
[ -n "$TOKEN" ] || die "GYM_TOKEN is empty in $ENV_FILE"
mkdir -p "$STATE_DIR"

run_gym() { # name host_port image
  docker rm -f "$1" >/dev/null 2>&1 || true
  # The container's own Chromium reaches its gym on 127.0.0.1:8787 inside the container; only the
  # host side of the port differs between live and canary.
  docker run -d --name "$1" --restart unless-stopped \
    --env-file "$ENV_FILE" \
    --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "$STATE_DIR:/var/lib/gym" \
    -p "127.0.0.1:$2:8787" \
    --shm-size=1g \
    "$3" >/dev/null
}

smoke() { # host_port
  local deadline=$((SECONDS + 240))
  until curl -fsS "http://127.0.0.1:$1/healthz" >/dev/null 2>&1; do
    [ $SECONDS -lt $deadline ] || return 1
    sleep 2
  done
  # Opens Excel, pings the bridge, reads A1 and checks the grid painted. Read-only, so it is safe
  # beside the live gym editing the same workbook.
  curl -fsS --max-time 240 "http://127.0.0.1:$1/s/$TOKEN/smoke"
}

if [ "${1:-}" = "--rollback" ]; then
  docker image inspect "$IMAGE:previous" >/dev/null 2>&1 || die "no previous image"
  run_gym gym-live "$LIVE_PORT" "$IMAGE:previous"
  log "rolled back to $IMAGE:previous"
  exit 0
fi

SHA=$(git -C "$REPO_ROOT" rev-parse --short HEAD)
log "building $IMAGE:$SHA"
docker build -q -f "$REPO_ROOT/gym/deploy/Dockerfile" -t "$IMAGE:$SHA" "$REPO_ROOT" >/dev/null || die "image build"

log "starting canary on :$CANARY_PORT"
run_gym gym-canary "$CANARY_PORT" "$IMAGE:$SHA"
if ! result=$(smoke "$CANARY_PORT"); then
  docker logs --tail 50 gym-canary || true
  docker rm -f gym-canary >/dev/null
  die "canary smoke check; the live gym is unchanged"
fi
log "canary ok: $result"
docker rm -f gym-canary >/dev/null

if docker image inspect "$IMAGE:live" >/dev/null 2>&1; then
  docker tag "$IMAGE:live" "$IMAGE:previous"
fi
docker tag "$IMAGE:$SHA" "$IMAGE:live"
docker rmi "$IMAGE:$SHA" >/dev/null
log "replacing live gym on :$LIVE_PORT"
run_gym gym-live "$LIVE_PORT" "$IMAGE:live"
if ! result=$(smoke "$LIVE_PORT"); then
  log "live gym failed its smoke check; rolling back"
  "$0" --rollback
  die "live smoke check after swap"
fi
log "deployed $SHA: $result"
docker image prune -f >/dev/null
