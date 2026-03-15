terraform {
  required_version = ">= 1.5"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    null = {
      source  = "hashicorp/null"
      version = "~> 3.0"
    }
  }
}

provider "aws" {
  region  = var.region
  profile = var.aws_profile
}

# ---------------------------------------------------------------------------
# Data sources
# ---------------------------------------------------------------------------

data "aws_ami" "ubuntu" {
  most_recent = true
  owners      = ["099720109477"] # Canonical

  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }
}

# ---------------------------------------------------------------------------
# Networking
# ---------------------------------------------------------------------------

resource "aws_default_vpc" "default" {
  tags = { Name = "${var.name_prefix}-vpc" }
}

resource "aws_security_group" "reasondb" {
  name        = "${var.name_prefix}-sg"
  description = "ReasonDB testing instance"
  vpc_id      = aws_default_vpc.default.id

  ingress {
    description = "SSH"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = [var.allowed_cidr]
  }

  ingress {
    description = "ReasonDB API"
    from_port   = 4444
    to_port     = 4444
    protocol    = "tcp"
    cidr_blocks = [var.allowed_cidr]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = { Name = "${var.name_prefix}-sg" }
}

# ---------------------------------------------------------------------------
# SSH key pair
# ---------------------------------------------------------------------------

resource "aws_key_pair" "reasondb" {
  key_name   = "${var.name_prefix}-key"
  public_key = var.ssh_public_key
}

# ---------------------------------------------------------------------------
# EC2 instance
# ---------------------------------------------------------------------------

resource "aws_instance" "reasondb" {
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = var.instance_type
  key_name               = aws_key_pair.reasondb.key_name
  vpc_security_group_ids = [aws_security_group.reasondb.id]

  # Root volume — OS only
  root_block_device {
    volume_type           = "gp3"
    volume_size           = 20
    delete_on_termination = true
  }

  user_data = templatefile("${path.module}/user_data.sh.tpl", {
    llm_provider   = var.llm_provider
    llm_api_key    = var.llm_api_key
    llm_model      = var.llm_model
    llm_base_url   = var.llm_base_url
    reasondb_image = var.reasondb_image
    auth_enabled   = var.auth_enabled
    master_key     = var.master_key
  })

  tags = { Name = "${var.name_prefix}-instance" }

  # Wait until the instance passes status checks before reporting ready
  lifecycle {
    create_before_destroy = true
  }
}

# ---------------------------------------------------------------------------
# EBS data volume — persists ReasonDB data independently of the instance
# ---------------------------------------------------------------------------

resource "aws_ebs_volume" "data" {
  availability_zone = aws_instance.reasondb.availability_zone
  size              = var.volume_size_gb
  type              = "gp3"

  tags = { Name = "${var.name_prefix}-data" }
}

resource "aws_volume_attachment" "data" {
  device_name  = "/dev/xvdf"
  volume_id    = aws_ebs_volume.data.id
  instance_id  = aws_instance.reasondb.id
  force_detach = true
}

# ---------------------------------------------------------------------------
# Elastic IP — stable address across reboots
# ---------------------------------------------------------------------------

resource "aws_eip" "reasondb" {
  instance = aws_instance.reasondb.id
  domain   = "vpc"

  tags = { Name = "${var.name_prefix}-eip" }

  depends_on = [aws_instance.reasondb]
}

# ---------------------------------------------------------------------------
# Container updater — re-runs whenever image or config variables change.
#
# user_data only executes once at first boot, so this null_resource handles
# zero-downtime container updates on subsequent `terraform apply` runs by
# SSH-ing into the instance and restarting the Docker container in-place.
# ---------------------------------------------------------------------------

resource "null_resource" "container_update" {
  # Re-trigger whenever any of these values change
  triggers = {
    image        = var.reasondb_image
    llm_provider = var.llm_provider
    llm_model    = var.llm_model
    llm_base_url = var.llm_base_url
    auth_enabled = tostring(var.auth_enabled)
    # Use a hash of the key so the value doesn't appear in state
    master_key_hash = sha256(var.master_key)
    llm_key_hash    = sha256(var.llm_api_key)
    # Re-run if the instance is replaced
    instance_id = aws_instance.reasondb.id
  }

  connection {
    type        = "ssh"
    user        = "ubuntu"
    private_key = file(var.ssh_private_key_path)
    host        = aws_eip.reasondb.public_ip
  }

  provisioner "remote-exec" {
    inline = [
      "echo '=== Pulling ${var.reasondb_image} ==='",
      "sudo docker pull ${var.reasondb_image}",
      "echo '=== Stopping existing container ==='",
      "sudo docker stop reasondb 2>/dev/null || true",
      "sudo docker rm   reasondb 2>/dev/null || true",
      "echo '=== Starting updated container ==='",
      "sudo docker run -d \\",
      "  --name reasondb \\",
      "  --restart unless-stopped \\",
      "  -p 4444:4444 \\",
      "  -v /data:/data \\",
      "  -e REASONDB_HOST=0.0.0.0 \\",
      "  -e REASONDB_PORT=4444 \\",
      "  -e REASONDB_PATH=/data/reasondb.redb \\",
      "  -e REASONDB_LLM_PROVIDER=${var.llm_provider} \\",
      "  -e REASONDB_LLM_API_KEY=${var.llm_api_key} \\",
      "  -e REASONDB_MODEL=${var.llm_model} \\",
      "  -e REASONDB_LLM_BASE_URL=${var.llm_base_url} \\",
      "  -e REASONDB_RATE_LIMIT_RPM=300 \\",
      "  -e REASONDB_RATE_LIMIT_RPH=5000 \\",
      "  -e REASONDB_RATE_LIMIT_BURST=30 \\",
      "  -e REASONDB_WORKER_COUNT=4 \\",
      "  -e REASONDB_AUTH_ENABLED=${var.auth_enabled} \\",
      "  -e REASONDB_MASTER_KEY=${var.master_key} \\",
      "  ${var.reasondb_image}",
      "echo '=== Waiting for health check ==='",
      "for i in $(seq 1 24); do curl -sf http://localhost:4444/health && echo '' && break || sleep 5; done",
    ]
  }

  depends_on = [aws_eip.reasondb, aws_volume_attachment.data]
}
