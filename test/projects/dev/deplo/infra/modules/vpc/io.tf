variable "project" {
  type = string
}
variable "envs" {
  type = list(string)
}
variable "region" {
  type = string
}
variable "dependencies" {
  type = list(string)
  default = []
}

output "networks" {
  value = {for k in keys(google_compute_network.vpc-network) : 
    k => google_compute_network.vpc-network[k].self_link
  }

}
