output "public_ip" {
  description = "Elastic IP address of the ReasonDB instance"
  value       = aws_eip.reasondb.public_ip
}

output "api_url" {
  description = "Base URL for the ReasonDB REST API"
  value       = "http://${aws_eip.reasondb.public_ip}:4444"
}

output "swagger_ui" {
  description = "Swagger UI — explore and test the API interactively"
  value       = "http://${aws_eip.reasondb.public_ip}:4444/swagger-ui/"
}

output "health_check" {
  description = "Health check endpoint"
  value       = "http://${aws_eip.reasondb.public_ip}:4444/health"
}

output "ssh_command" {
  description = "SSH command to connect to the instance"
  value       = "ssh ubuntu@${aws_eip.reasondb.public_ip}"
}

output "instance_id" {
  description = "EC2 instance ID"
  value       = aws_instance.reasondb.id
}

output "ebs_volume_id" {
  description = "EBS data volume ID (keep this — it holds your ReasonDB data)"
  value       = aws_ebs_volume.data.id
}
