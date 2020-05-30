//
// tfvars
//
variable "root_domain" {}
variable "dns_zone" {}
variable "dns_zone_project" {}
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

//
// terraform settings
//
terraform {
  backend "gcs" {
    //configured by config/backend 
  }
}
provider "google" {
  project     = var.project_id
  region      = var.region
  version     = "~> 3.22.0"
}
provider "google-beta" {
  project     = var.project_id
  region      = var.region
  version     = "~> 3.22.0"
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

  dependencies = [
    module.api.ready
  ]
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
  dns_zone_project = var.dns_zone_project
  default_backend_url = module.storage.bucket_404_url

  dependencies = [
    module.api.ready
  ]
}
