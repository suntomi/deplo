//
// tfvars
//
variable "gcp" {}

locals {
  root_domain = var.gcp.root_domain
  dns_zone = var.gcp.dns_zone
  dns_zone_project = var.gcp.dns_zone_project
  project_id = var.gcp.project_id
  region = var.gcp.region
  envs = var.gcp.envs
  lbs = var.gcp.lbs
  resource_prefix = var.gcp.resource_prefix
}


//
// terraform settings
//
provider "google" {
  project     = local.project_id
  region      = local.region
  version     = "~> 3.22.0"
}
provider "google-beta" {
  project     = local.project_id
  region      = local.region
  version     = "~> 3.22.0"
}

//
// api
//
module "api" {
  source = "./gcp/modules/api"
}

//
// storage
//
module "storage" {
  source = "./gcp/modules/storage"
  prefix = local.resource_prefix

  dependencies = [
    module.api.ready
  ]
}

//
// vpc
//
module "vpc" {
  source = "./gcp/modules/vpc"

  envs = local.envs
  prefix = local.resource_prefix
  region = local.region

  dependencies = [
    module.api.ready
  ]
}

//
// lb 
//
module "lb" {
  source = "./gcp/modules/lb"

  envs = local.envs
  prefix = local.resource_prefix
  root_domain = "${local.resource_prefix}.${local.root_domain}"
  dns_zone = local.dns_zone
  dns_zone_project = local.dns_zone_project
  default_backend_url = module.storage.bucket_404_url

  dependencies = [
    module.api.ready
  ]
}
