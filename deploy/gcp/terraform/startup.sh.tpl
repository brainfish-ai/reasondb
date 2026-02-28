#!/bin/bash
set -euo pipefail

# ── Install Docker ────────────────────────────────────────────────────────────
apt-get update -y
apt-get install -y ca-certificates curl gnupg lsb-release

install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg \
  | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg

echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
  https://download.docker.com/linux/debian $(lsb_release -cs) stable" \
  > /etc/apt/sources.list.d/docker.list

apt-get update -y
apt-get install -y docker-ce docker-ce-cli containerd.io
systemctl enable --now docker

# ── Format and mount the persistent data disk ─────────────────────────────────
# GCE attaches additional disks as /dev/disk/by-id/google-<device-name>
DATA_DISK="/dev/disk/by-id/google-reasondb-data"

for i in $(seq 1 12); do
  [ -b "$DATA_DISK" ] && break
  sleep 5
done

if ! blkid "$DATA_DISK" | grep -q ext4; then
  mkfs.ext4 "$DATA_DISK"
fi

mkdir -p /data
mount "$DATA_DISK" /data

BLKID=$(blkid -s UUID -o value "$DATA_DISK")
echo "UUID=$BLKID /data ext4 defaults,nofail 0 2" >> /etc/fstab

# ── Start ReasonDB ────────────────────────────────────────────────────────────
docker pull ${reasondb_image}

docker run -d \
  --name reasondb \
  --restart unless-stopped \
  -p 4444:4444 \
  -v /data:/data \
  -e REASONDB_HOST="0.0.0.0" \
  -e REASONDB_PORT="4444" \
  -e REASONDB_PATH="/data/reasondb.redb" \
  -e REASONDB_LLM_PROVIDER="${llm_provider}" \
  -e REASONDB_LLM_API_KEY="${llm_api_key}" \
  -e REASONDB_MODEL="${llm_model}" \
  -e REASONDB_LLM_BASE_URL="${llm_base_url}" \
  -e REASONDB_RATE_LIMIT_RPM="300" \
  -e REASONDB_RATE_LIMIT_RPH="5000" \
  -e REASONDB_RATE_LIMIT_BURST="30" \
  -e REASONDB_WORKER_COUNT="4" \
  ${reasondb_image}
