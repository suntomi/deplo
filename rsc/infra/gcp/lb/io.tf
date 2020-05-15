variable "project" {
  type = string
}
variable "root_domain" {
  type = string
}
variable "zone_name" {
  type = string
}
variable "default_backend_url" {
  type = string
}
variable "env" {
  type = string
}
variable "ip_version" {
  type = string
  default = "IPV4"
}

output "dns_name" {
  value = "${google_dns_record_set.a.name}"
}
output "ip_address" {
  value = "${google_compute_global_address.default.address}"
}

