resource "null_resource" "dependency_getter" {
  provisioner "local-exec" {
    command = "echo ${length(var.dependencies)}"
  }
}

resource "google_storage_bucket" "bucket_404" {
  name     = "${var.prefix}-service-404"
}

resource "google_compute_backend_bucket" "bucket_404" {
  name        = "${var.prefix}-backend-bucket-404"
  bucket_name = google_storage_bucket.bucket_404.name
  enable_cdn  = true
}

resource "google_storage_bucket_acl" "public_bucket_404" {
  bucket = google_storage_bucket.bucket_404.name
  predefined_acl = "publicRead"
}

resource "google_storage_bucket" "custom_buckets" {
  for_each = toset(var.custom_buckets)
  name = "${var.prefix}-${each.value}"
}

resource "google_compute_backend_bucket" "custom_buckets" {
  for_each    = toset(var.custom_buckets)
  name        = "${google_storage_bucket.custom_buckets[each.value].name}-backend"
  bucket_name = google_storage_bucket.custom_buckets[each.value].name
  enable_cdn  = true

  depends_on = [
    null_resource.dependency_getter
  ]
}

resource "google_storage_bucket_acl" "custom_buckets" {
  for_each       = toset(var.custom_buckets)
  bucket         = google_storage_bucket.custom_buckets[each.value].name
  predefined_acl = lookup(var.custom_buckets_type_acl, element(split("-", each.value), 1), "publicRead")
}

locals {
  flatten_custom_access_controls = flatten([ 
    for c in var.custom_access_controls: [
      for b in c.buckets: "${c.account}/${c.role}/${b}"
    ]
  ])
}

resource "google_storage_bucket_access_control" "custom_access_controls" {
  for_each       = toset(local.flatten_custom_access_controls)
  entity = "user-${element(split("/", each.value), 0)}"
  role   = element(split("/", each.value), 1)
  bucket = "${var.prefix}-${element(split("/", each.value), 2)}"
}
