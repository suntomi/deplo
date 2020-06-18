//
// tfvars
//
variable "root_domain" {}
variable "dns_zone" {}
variable "dns_zone_project" {}
variable "project_id" {}
variable "region" {}
variable "resource_prefix" {}
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

locals {
  resource_prefix = "${length(var.resource_prefix) > 0 ? var.resource_prefix : var.project_id}"
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
  prefix = local.resource_prefix

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
  prefix = local.resource_prefix
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
  prefix = local.resource_prefix
  root_domain = "${local.resource_prefix}.${var.root_domain}"
  dns_zone = var.dns_zone
  dns_zone_project = var.dns_zone_project
  default_backend_url = module.storage.bucket_404_url

  dependencies = [
    module.api.ready
  ]
}
