variable "root_domain" {}
variable "project_id" {}
variable "region" {}
variable "envs" {
  type = list(string)
}
variable "predefined_zone" {
  type = string
  default = ""
}

terraform {
  backend "gcs" {
    //configured by config/backend 
  }
}

module "dns" {
  source = "./modules/dns"

  project = "${var.project_id}"
  root_domain = "${var.root_domain}"
  predefined_zone = "${var.predefined_zone}"
}

//
// vpc
//
module "vpc" {
  for_each  = "${toset(var.envs)}"
  source = "./modules/vpc"

  env = "${each.value}"
  project = "${var.project_id}"
  region = "${var.region}"
}

//
// lb 
//
module "lb" {
  for_each  = "${toset(var.envs)}"
  source = "./modules/lb"

  env = "${each.value}"
  project = "${var.project_id}"
  root_domain = "${var.root_domain}"
  zone_name = "${module.dns.zone_name}"
  default_backend_url = "${module.storage.bucket_404_url}"
}
