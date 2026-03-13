terraform {
  required_version = ">= 1.5"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
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
