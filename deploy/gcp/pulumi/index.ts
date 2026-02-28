import * as pulumi from "@pulumi/pulumi";
import * as gcp from "@pulumi/gcp";

const config    = new pulumi.Config();
const gcpConfig = new pulumi.Config("gcp");

// ── Configuration ─────────────────────────────────────────────────────────────
const namePrefix    = config.get("namePrefix")    ?? "reasondb-testing";
const machineType   = config.get("machineType")   ?? "e2-medium";
const diskSizeGb    = config.getNumber("diskSizeGb") ?? 20;
const allowedCidr   = config.get("allowedCidr")   ?? "0.0.0.0/0";
const sshPublicKey  = config.require("sshPublicKey");
const sshUser       = config.get("sshUser")       ?? "reasondb";
const llmProvider   = config.get("llmProvider")   ?? "openai";
const llmApiKey     = config.requireSecret("llmApiKey");
const llmModel      = config.get("llmModel")      ?? "";
const llmBaseUrl    = config.get("llmBaseUrl")    ?? "";
const reasondbImage = config.get("reasondbImage") ?? "ajainvivek/reasondb:latest";
const zone          = gcpConfig.get("zone")       ?? "us-central1-a";
const region        = gcpConfig.get("region")     ?? "us-central1";

// ── Static External IP ─────────────────────────────────────────────────────────
const staticIp = new gcp.compute.Address(`${namePrefix}-ip`, {
    region: region,
});

// ── Firewall Rule ──────────────────────────────────────────────────────────────
const firewall = new gcp.compute.Firewall(`${namePrefix}-fw`, {
    network: "default",
    allows: [
        { protocol: "tcp", ports: ["22", "4444"] },
    ],
    sourceRanges: [allowedCidr],
    targetTags:   [namePrefix],
});

// ── Persistent Data Disk ───────────────────────────────────────────────────────
const dataDisk = new gcp.compute.Disk(`${namePrefix}-data`, {
    zone: zone,
    type: "pd-ssd",
    size: diskSizeGb,
});

// ── Startup Script ─────────────────────────────────────────────────────────────
const startupScript = pulumi.all([llmApiKey]).apply(([apiKey]) => `#!/bin/bash
set -euo pipefail

# Install Docker
apt-get update -y
apt-get install -y ca-certificates curl gnupg lsb-release
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" > /etc/apt/sources.list.d/docker.list
apt-get update -y
apt-get install -y docker-ce docker-ce-cli containerd.io
systemctl enable --now docker

# Wait for and mount the persistent data disk
DATA_DISK="/dev/disk/by-id/google-${namePrefix}-data"
for i in $(seq 1 12); do [ -b "$DATA_DISK" ] && break; sleep 5; done
if ! blkid "$DATA_DISK" | grep -q ext4; then mkfs.ext4 "$DATA_DISK"; fi
mkdir -p /data
mount "$DATA_DISK" /data
BLKID=$(blkid -s UUID -o value "$DATA_DISK")
echo "UUID=$BLKID /data ext4 defaults,nofail 0 2" >> /etc/fstab

# Start ReasonDB
docker pull ${reasondbImage}
docker run -d \\
  --name reasondb \\
  --restart unless-stopped \\
  -p 4444:4444 \\
  -v /data:/data \\
  -e REASONDB_HOST="0.0.0.0" \\
  -e REASONDB_PORT="4444" \\
  -e REASONDB_PATH="/data/reasondb.redb" \\
  -e REASONDB_LLM_PROVIDER="${llmProvider}" \\
  -e REASONDB_LLM_API_KEY="${apiKey}" \\
  -e REASONDB_MODEL="${llmModel}" \\
  -e REASONDB_LLM_BASE_URL="${llmBaseUrl}" \\
  -e REASONDB_RATE_LIMIT_RPM="300" \\
  -e REASONDB_RATE_LIMIT_RPH="5000" \\
  -e REASONDB_RATE_LIMIT_BURST="30" \\
  -e REASONDB_WORKER_COUNT="4" \\
  ${reasondbImage}
`);

// ── GCE Instance ───────────────────────────────────────────────────────────────
const instance = new gcp.compute.Instance(namePrefix, {
    machineType: machineType,
    zone:        zone,
    tags:        [namePrefix],

    bootDisk: {
        initializeParams: {
            image: "debian-cloud/debian-12",
            size:  20,
            type:  "pd-ssd",
        },
    },

    attachedDisks: [{
        source:     dataDisk.selfLink,
        deviceName: `${namePrefix}-data`,
        mode:       "READ_WRITE",
    }],

    networkInterfaces: [{
        network: "default",
        accessConfigs: [{
            natIp: staticIp.address,
        }],
    }],

    metadata: {
        "ssh-keys":       pulumi.interpolate`${sshUser}:${sshPublicKey}`,
        "startup-script": startupScript,
    },

    serviceAccount: {
        scopes: ["cloud-platform"],
    },
}, { dependsOn: [firewall] });

// ── Outputs ────────────────────────────────────────────────────────────────────
export const publicIp          = staticIp.address;
export const apiUrl            = pulumi.interpolate`http://${staticIp.address}:4444`;
export const swaggerUi         = pulumi.interpolate`http://${staticIp.address}:4444/swagger-ui/`;
export const healthCheck       = pulumi.interpolate`http://${staticIp.address}:4444/health`;
export const sshCommand        = pulumi.interpolate`ssh ${sshUser}@${staticIp.address}`;
export const instanceName      = instance.name;
export const persistentDiskName = dataDisk.name;
