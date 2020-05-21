variable "root_domain" {}
variable "dns_zone" {}
variable "project_id" {}
variable "region" {}
variable "bucket_prefix" {}
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

//
// api
//
module "api" {
  source = "./modules/api"
}

//
// storage
//
module "storage" {
  source = "./modules/storage"
  project = var.project_id
  bucket_prefix = var.bucket_prefix
}

//
// vpc
//
module "vpc" {
  source = "./modules/vpc"

  envs = var.envs
  project = var.project_id
  region = var.region

  dependencies = [
    module.api.ready
  ]
}

//
// lb 
//
module "lb" {
  source = "./modules/lb"

  envs = var.envs
  project = var.project_id
  root_domain = "${var.project_id}.${var.root_domain}"
  dns_zone = var.dns_zone
  default_backend_url = module.storage.bucket_404_url
}
