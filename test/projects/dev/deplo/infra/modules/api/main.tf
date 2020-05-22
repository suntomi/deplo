locals {
  basic_services = [
    "compute.googleapis.com",
    "servicenetworking.googleapis.com"
  ]
}

resource "google_project_service" "project" {
  for_each = toset(concat(local.basic_services, var.services))
  service = each.value
}

resource "null_resource" "dependency_setter" {
  depends_on = [
    google_project_service.project
  ]
}
