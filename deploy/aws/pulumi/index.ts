import * as pulumi from "@pulumi/pulumi";
import * as aws from "@pulumi/aws";

const config = new pulumi.Config();
const awsConfig = new pulumi.Config("aws");

// ── Configuration ─────────────────────────────────────────────────────────────
const region        = awsConfig.get("region") ?? "us-east-1";
const namePrefix    = config.get("namePrefix") ?? "reasondb-testing";
const instanceType  = config.get("instanceType") ?? "t3.medium";
const volumeSizeGb  = config.getNumber("volumeSizeGb") ?? 20;
const allowedCidr   = config.get("allowedCidr") ?? "0.0.0.0/0";
const sshPublicKey  = config.require("sshPublicKey");
const llmProvider   = config.get("llmProvider") ?? "openai";
const llmApiKey     = config.requireSecret("llmApiKey");
const llmModel      = config.get("llmModel") ?? "";
const llmBaseUrl    = config.get("llmBaseUrl") ?? "";
const reasondbImage = config.get("reasondbImage") ?? "ajainvivek/reasondb:latest";

// ── AMI lookup — Ubuntu 22.04 LTS ─────────────────────────────────────────────
const ami = aws.ec2.getAmi({
    mostRecent: true,
    owners: ["099720109477"], // Canonical
    filters: [
        { name: "name", values: ["ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*"] },
        { name: "virtualization-type", values: ["hvm"] },
    ],
});

// ── Security Group ─────────────────────────────────────────────────────────────
const securityGroup = new aws.ec2.SecurityGroup(`${namePrefix}-sg`, {
    description: "ReasonDB testing instance",
    ingress: [
        { description: "SSH",           fromPort: 22,   toPort: 22,   protocol: "tcp", cidrBlocks: [allowedCidr] },
        { description: "ReasonDB API",  fromPort: 4444, toPort: 4444, protocol: "tcp", cidrBlocks: [allowedCidr] },
    ],
    egress: [
        { fromPort: 0, toPort: 0, protocol: "-1", cidrBlocks: ["0.0.0.0/0"] },
    ],
    tags: { Name: `${namePrefix}-sg` },
});

// ── SSH Key Pair ───────────────────────────────────────────────────────────────
const keyPair = new aws.ec2.KeyPair(`${namePrefix}-key`, {
    keyName:   `${namePrefix}-key`,
    publicKey: sshPublicKey,
});

// ── User Data — runs on first boot ─────────────────────────────────────────────
const userData = pulumi.all([llmApiKey]).apply(([apiKey]) => `#!/bin/bash
set -euo pipefail

# Install Docker
apt-get update -y
apt-get install -y ca-certificates curl gnupg lsb-release
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" > /etc/apt/sources.list.d/docker.list
apt-get update -y
apt-get install -y docker-ce docker-ce-cli containerd.io
systemctl enable --now docker

# Wait for EBS volume
for i in $(seq 1 12); do [ -b /dev/xvdf ] && break; sleep 5; done
if ! blkid /dev/xvdf | grep -q ext4; then mkfs.ext4 /dev/xvdf; fi
mkdir -p /data
mount /dev/xvdf /data
BLKID=$(blkid -s UUID -o value /dev/xvdf)
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

// ── EC2 Instance ───────────────────────────────────────────────────────────────
const instance = new aws.ec2.Instance(`${namePrefix}-instance`, {
    ami:                  ami.then(a => a.id),
    instanceType:         instanceType,
    keyName:              keyPair.keyName,
    vpcSecurityGroupIds:  [securityGroup.id],
    userData:             userData,
    rootBlockDevice: {
        volumeType:          "gp3",
        volumeSize:          20,
        deleteOnTermination: true,
    },
    tags: { Name: `${namePrefix}-instance` },
});

// ── EBS Data Volume ────────────────────────────────────────────────────────────
const dataVolume = new aws.ebs.Volume(`${namePrefix}-data`, {
    availabilityZone: instance.availabilityZone,
    size:             volumeSizeGb,
    type:             "gp3",
    tags: { Name: `${namePrefix}-data` },
});

new aws.ec2.VolumeAttachment(`${namePrefix}-vol-attach`, {
    deviceName:  "/dev/xvdf",
    volumeId:    dataVolume.id,
    instanceId:  instance.id,
    forceDetach: true,
});

// ── Elastic IP ─────────────────────────────────────────────────────────────────
const eip = new aws.ec2.Eip(`${namePrefix}-eip`, {
    instance: instance.id,
    domain:   "vpc",
    tags: { Name: `${namePrefix}-eip` },
});

// ── Outputs ────────────────────────────────────────────────────────────────────
export const publicIp      = eip.publicIp;
export const apiUrl        = pulumi.interpolate`http://${eip.publicIp}:4444`;
export const swaggerUi     = pulumi.interpolate`http://${eip.publicIp}:4444/swagger-ui/`;
export const healthCheck   = pulumi.interpolate`http://${eip.publicIp}:4444/health`;
export const sshCommand    = pulumi.interpolate`ssh ubuntu@${eip.publicIp}`;
export const instanceId    = instance.id;
export const ebsVolumeId   = dataVolume.id;
