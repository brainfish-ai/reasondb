output "public_ip" {
  description = "Static external IP address of the ReasonDB instance"
  value       = google_compute_address.reasondb.address
}

output "api_url" {
  description = "Base URL for the ReasonDB REST API"
  value       = "http://${google_compute_address.reasondb.address}:4444"
}

output "swagger_ui" {
  description = "Swagger UI — explore and test the API interactively"
  value       = "http://${google_compute_address.reasondb.address}:4444/swagger-ui/"
}

output "health_check" {
  description = "Health check endpoint"
  value       = "http://${google_compute_address.reasondb.address}:4444/health"
}

output "ssh_command" {
  description = "SSH command to connect to the instance"
  value       = "ssh ${var.ssh_user}@${google_compute_address.reasondb.address}"
}

output "instance_name" {
  description = "GCE instance name"
  value       = google_compute_instance.reasondb.name
}

output "persistent_disk_name" {
  description = "Persistent disk name (holds your ReasonDB data)"
  value       = google_compute_disk.data.name
}
