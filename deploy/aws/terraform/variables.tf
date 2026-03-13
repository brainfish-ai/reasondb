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
  description = "SSH public key material (contents of ~/.ssh/id_rsa.pub or similar)"
  type        = string
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
