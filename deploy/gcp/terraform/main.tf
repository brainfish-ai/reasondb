terraform {
  required_version = ">= 1.5"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
  }
}

provider "google" {
  project = var.project_id
  region  = var.region
  zone    = var.zone
}

# ---------------------------------------------------------------------------
# Static external IP — survives instance restarts
# ---------------------------------------------------------------------------

resource "google_compute_address" "reasondb" {
  name   = "${var.name_prefix}-ip"
  region = var.region
}

# ---------------------------------------------------------------------------
# Firewall — allow SSH and ReasonDB API port
# ---------------------------------------------------------------------------

resource "google_compute_firewall" "reasondb" {
  name    = "${var.name_prefix}-fw"
  network = "default"

  allow {
    protocol = "tcp"
    ports    = ["22", "4444"]
  }

  source_ranges = [var.allowed_cidr]
  target_tags   = [var.name_prefix]
}

# ---------------------------------------------------------------------------
# Persistent data disk — holds /data independently of the boot disk
# ---------------------------------------------------------------------------

resource "google_compute_disk" "data" {
  name = "${var.name_prefix}-data"
  type = "pd-ssd"
  zone = var.zone
  size = var.disk_size_gb
}

# ---------------------------------------------------------------------------
# GCE instance
# ---------------------------------------------------------------------------

resource "google_compute_instance" "reasondb" {
  name         = var.name_prefix
  machine_type = var.machine_type
  zone         = var.zone
  tags         = [var.name_prefix]

  boot_disk {
    initialize_params {
      image = "debian-cloud/debian-12"
      size  = 20
      type  = "pd-ssd"
    }
  }

  attached_disk {
    source      = google_compute_disk.data.self_link
    device_name = "reasondb-data"
    mode        = "READ_WRITE"
  }

  network_interface {
    network = "default"
    access_config {
      nat_ip = google_compute_address.reasondb.address
    }
  }

  metadata = {
    ssh-keys       = "${var.ssh_user}:${var.ssh_public_key}"
    startup-script = templatefile("${path.module}/startup.sh.tpl", {
      llm_provider   = var.llm_provider
      llm_api_key    = var.llm_api_key
      llm_model      = var.llm_model
      llm_base_url   = var.llm_base_url
      reasondb_image = var.reasondb_image
    })
  }

  service_account {
    scopes = ["cloud-platform"]
  }
}
