variable "project" {
  type = string
}
variable "root_domain" {
  type = string
}
variable "dns_zone" {
  type = string
}
variable "dns_zone_project" {
  type = string
}
variable "default_backend_url" {
  type = string
}
variable "envs" {
  type = list(string)
}
variable "ip_version" {
  type = string
  default = "IPV4"
}
variable "dependencies" {
  type = list(string)
  default = []
}

output "dns_names" {
  value = {for k in keys(google_dns_record_set.a) : 
    k => google_dns_record_set.a[k].name
  }
}
output "ip_addresses" {
  value = {for k in keys(google_compute_global_address.default) : 
    k => google_compute_global_address.default[k].name
  }
}

