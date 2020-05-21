variable "project" {
  type = string
}
variable "root_domain" {
  type = string
}
variable "predefined_zone" {
  type = string 
  default = ""
}

output "zone_name" {
  value = "${length(var.predefined_zone) > 0 ? var.predefined_zone : google_dns_managed_zone.root[0].name}"
}