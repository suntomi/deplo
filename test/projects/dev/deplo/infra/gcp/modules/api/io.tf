variable "services" {
  type = list(string)
  default = []
}

output "ready" {
  value = "${null_resource.dependency_setter.id}"
}
