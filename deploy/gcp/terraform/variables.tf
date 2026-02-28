variable "project_id" {
  description = "GCP project ID to deploy into"
  type        = string
}

variable "region" {
  description = "GCP region (e.g. us-central1)"
  type        = string
  default     = "us-central1"
}

variable "zone" {
  description = "GCP zone (e.g. us-central1-a)"
  type        = string
  default     = "us-central1-a"
}

variable "machine_type" {
  description = "GCE machine type (e2-medium recommended for testing)"
  type        = string
  default     = "e2-medium"
}

variable "disk_size_gb" {
  description = "Size of the persistent data disk in GB"
  type        = number
  default     = 20
}

variable "ssh_public_key" {
  description = "SSH public key material in OpenSSH format (contents of ~/.ssh/id_rsa.pub)"
  type        = string
}

variable "ssh_user" {
  description = "OS username for SSH access"
  type        = string
  default     = "reasondb"
}

variable "allowed_cidr" {
  description = "Source IP range allowed to reach port 4444 and 22. Use your IP (e.g. 1.2.3.4/32) or 0.0.0.0/0 for open access."
  type        = string
  default     = "0.0.0.0/0"
}

variable "llm_provider" {
  description = "LLM provider name (openai, anthropic, gemini, cohere, bedrock, vertex, ollama)"
  type        = string
  default     = "openai"
}

variable "llm_api_key" {
  description = "API key for the LLM provider (stored as sensitive)"
  type        = string
  sensitive   = true
}

variable "llm_model" {
  description = "Model name override (leave empty to use provider default)"
  type        = string
  default     = ""
}

variable "llm_base_url" {
  description = "Custom LLM base URL (for Ollama or compatible endpoints)"
  type        = string
  default     = ""
}

variable "reasondb_image" {
  description = "Docker image to pull and run"
  type        = string
  default     = "ajainvivek/reasondb:latest"
}

variable "name_prefix" {
  description = "Prefix applied to all resource names"
  type        = string
  default     = "reasondb-testing"
}
