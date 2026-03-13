#!/bin/bash
set -euo pipefail

# ── Install Docker ────────────────────────────────────────────────────────────
apt-get update -y
apt-get install -y ca-certificates curl gnupg lsb-release

install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
  | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg

echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
  https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" \
  > /etc/apt/sources.list.d/docker.list

apt-get update -y
apt-get install -y docker-ce docker-ce-cli containerd.io
systemctl enable --now docker

# ── Format and mount the EBS data volume ─────────────────────────────────────
# Nitro instances expose the volume as /dev/nvme1n1; older instances use /dev/xvdf.
# Wait up to 120 s for either name to appear.
EBS_DEV=""
for i in $(seq 1 24); do
  if [ -b /dev/nvme1n1 ]; then
    EBS_DEV=/dev/nvme1n1
    break
  elif [ -b /dev/xvdf ]; then
    EBS_DEV=/dev/xvdf
    break
  fi
  sleep 5
done

if [ -z "$EBS_DEV" ]; then
  echo "WARNING: EBS data volume not found after 120s — storing data on root volume" >&2
else
  if ! blkid "$EBS_DEV" | grep -q ext4; then
    mkfs.ext4 "$EBS_DEV"
  fi

  mkdir -p /data
  mount "$EBS_DEV" /data

  # Persist the mount across reboots
  BLKID=$(blkid -s UUID -o value "$EBS_DEV")
  echo "UUID=$BLKID /data ext4 defaults,nofail 0 2" >> /etc/fstab
fi

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
