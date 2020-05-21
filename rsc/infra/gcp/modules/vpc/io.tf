variable "project" {
  type = string
}
variable "env" {
  type = string
}
variable "region" {
  type = string
}

output "network" {
  value = "${google_compute_network.vpc-network.self_link}"
}
