variable "project" {
  type = string
}
variable "bucket_prefix" {
  type = string
  default = ""
}
variable "custom_buckets" {
  type = list(string)
  default = []
}
variable "custom_access_controls" {
  type = list(object({
    account = string,
    role = string,
    buckets = list(string)
  }))
  default = []
}
variable "custom_buckets_type_acl" {
  type = map(string)
  default = {}
}
variable "dependencies" {
  type = list(string)
  default = []
}

output "bucket_404_url" {
  value = "${google_compute_backend_bucket.bucket_404.self_link}"
}
