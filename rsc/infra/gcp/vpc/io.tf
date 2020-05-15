variable "project" {
  type = string
}
variable "env" {
  type = string
}
variable "region" {
  type = string
}
variable "extra_service_ports" {
  type = list(number)
  default = []
}


output "network" {
  value = "${google_compute_network.vpc-network.self_link}"
}
