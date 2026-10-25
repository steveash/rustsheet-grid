#!/usr/bin/env bash
# One-time setup of the gym host, run as the user that will own the gym (in the docker group):
#   curl -fsSL https://raw.githubusercontent.com/steveash/rustsheet-grid/main/gym/deploy/install.sh | bash
# Clones the repo, installs a systemd timer that runs updater.sh every 5 minutes, and runs it once.
# Needs sudo for /opt, /var/lib and the systemd units.
set -euo pipefail

CHECKOUT=/opt/rustsheet-grid
STATE_DIR=/var/lib/rustsheet-gym
ENV_FILE=/etc/rustsheet-gym/gym.env
USER_NAME=$(id -un)

id -nG | grep -qw docker || { echo "add $USER_NAME to the docker group first (sudo usermod -aG docker $USER_NAME, then log in again)"; exit 1; }
[ -r "$ENV_FILE" ] || { echo "create $ENV_FILE first (see gym/deploy/gym.env.example)"; exit 1; }

sudo mkdir -p "$CHECKOUT" "$STATE_DIR"
sudo chown "$USER_NAME": "$CHECKOUT" "$STATE_DIR"
[ -d "$CHECKOUT/.git" ] || git clone --quiet https://github.com/steveash/rustsheet-grid "$CHECKOUT"

sudo tee /etc/systemd/system/rustsheet-gym-updater.service >/dev/null <<EOF
[Unit]
Description=Deploy the rustsheet gym when gym/ changes on main
After=network-online.target docker.service
Wants=network-online.target

[Service]
Type=oneshot
User=$USER_NAME
ExecStart=$CHECKOUT/gym/deploy/updater.sh
EOF

sudo tee /etc/systemd/system/rustsheet-gym-updater.timer >/dev/null <<EOF
[Unit]
Description=Check for gym updates every 5 minutes

[Timer]
OnBootSec=1min
OnUnitActiveSec=5min

[Install]
WantedBy=timers.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now rustsheet-gym-updater.timer
echo "installed; first deploy running now (follow it with: journalctl -fu rustsheet-gym-updater)"
sudo systemctl start --no-block rustsheet-gym-updater.service
