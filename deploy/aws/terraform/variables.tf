variable "region" {
  description = "AWS region to deploy into"
  type        = string
  default     = "us-east-1"
}

variable "aws_profile" {
  description = "AWS CLI profile to use (e.g. an SSO profile configured via 'aws configure sso')"
  type        = string
  default     = "default"
}

variable "instance_type" {
  description = "EC2 instance type (t3.medium recommended for testing)"
  type        = string
  default     = "t3.medium"
}

variable "volume_size_gb" {
  description = "Size of the EBS data volume in GB"
  type        = number
  default     = 20
}

variable "ssh_public_key" {
  description = "SSH public key material (contents of ~/.ssh/id_ed25519.pub or similar)"
  type        = string
}

variable "ssh_private_key_path" {
  description = "Path to the SSH private key file used by the container updater provisioner (e.g. ~/.ssh/id_ed25519)"
  type        = string
  default     = "~/.ssh/id_ed25519"
}

variable "allowed_cidr" {
  description = "CIDR block allowed to reach port 4444 and 22. Use your IP (e.g. 1.2.3.4/32) or 0.0.0.0/0 for open access."
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

variable "auth_enabled" {
  description = "Enable API key authentication (recommended for production)"
  type        = bool
  default     = false
}

variable "master_key" {
  description = "Master admin key for the ReasonDB instance (required when auth_enabled = true)"
  type        = string
  sensitive   = true
  default     = ""
}
