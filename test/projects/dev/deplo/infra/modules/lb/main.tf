resource "null_resource" "dependency_getter" {
  provisioner "local-exec" {
    command = "echo ${length(var.dependencies)}"
  }
}

resource "google_dns_record_set" "a" {
  project = var.dns_zone_project
  for_each   = toset(var.envs)
  name = "${each.value}.${var.root_domain}."
  type = "A"
  ttl  = 300

  managed_zone = var.dns_zone

  rrdatas = [google_compute_global_address.default[each.value].address]

  depends_on = [google_compute_global_address.default]
}

resource "google_dns_record_set" "caa" {
  project = var.dns_zone_project
  for_each   = toset(var.envs)
  name = "${each.value}.${var.root_domain}."
  type = "CAA"
  ttl  = 300

  managed_zone = var.dns_zone

  rrdatas = ["0 issue \"letsencrypt.org\""]
}

resource "google_compute_global_forwarding_rule" "default" {
  for_each   = toset(var.envs)
  name       = "${var.project}-${each.value}-https"
  target     = google_compute_target_https_proxy.default[each.value].self_link
  ip_address = google_compute_global_address.default[each.value].address
  port_range = "443"
  depends_on = [google_compute_global_address.default]
}

resource "google_compute_global_address" "default" {
  for_each   = toset(var.envs)
  name       = "${var.project}-${each.value}-global-address"
  ip_version = var.ip_version

  depends_on = [
    null_resource.dependency_getter
  ]
}

# HTTPS proxy  when ssl is true
resource "google_compute_target_https_proxy" "default" {
  for_each   = toset(var.envs)
  name             = "${var.project}-${each.value}-https-proxy"
  url_map          = google_compute_url_map.default[each.value].self_link
  ssl_certificates = [google_compute_managed_ssl_certificate.default[each.value].self_link]
}

resource "google_compute_managed_ssl_certificate" "default" {
  provider = google-beta

  for_each = toset(var.envs)
  name = "${var.project}-${each.value}-cert"
  managed {
    domains = [google_dns_record_set.a[each.value].name]
  }
}

resource "google_compute_url_map" "default" {
  for_each        = toset(var.envs)
  name            = "${var.project}-${each.value}-url-map"
  default_service = var.default_backend_url

  lifecycle {
    // configured by external script
    ignore_changes = [host_rule, path_matcher, default_service]
  }
}
