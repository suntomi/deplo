locals {
  namespace = "${var.project}-${var.env}"
  domain_name = "${var.env}.${var.root_domain}"
}

resource "google_dns_record_set" "a" {
  name = "${local.domain_name}."
  type = "A"
  ttl  = 300

  managed_zone = "${var.zone_name}"

  rrdatas = ["${google_compute_global_address.default.address}"]

  depends_on = ["google_compute_global_address.default"]
}

resource "google_dns_record_set" "caa" {
  name = "${local.domain_name}."
  type = "CAA"
  ttl  = 300

  managed_zone = "${var.zone_name}"

  rrdatas = ["0 issue \"letsencrypt.org\""]
}

resource "google_compute_global_forwarding_rule" "default" {
  name       = "${local.namespace}-https"
  target     = "${google_compute_target_https_proxy.default.self_link}"
  ip_address = "${google_compute_global_address.default.address}"
  port_range = "443"
  depends_on = ["google_compute_global_address.default"]
}

resource "google_compute_global_address" "default" {
  name       = "${local.namespace}-global-address"
  ip_version = "${var.ip_version}"
}

# HTTPS proxy  when ssl is true
resource "google_compute_target_https_proxy" "default" {
  name             = "${local.namespace}-https-proxy"
  url_map          = "${google_compute_url_map.default.self_link}"
  ssl_certificates = ["${google_compute_managed_ssl_certificate.default.self_link}"]
}

resource "google_compute_managed_ssl_certificate" "default" {
  provider = "google-beta"

  name = "${local.namespace}-cert"

  managed {
    domains = ["${local.domain_name}."]
  }
}

resource "google_compute_url_map" "default" {
  name            = "${local.namespace}-url-map"
  default_service = "${var.default_backend_url}"

  lifecycle {
    // configured by external script
    ignore_changes = ["host_rule", "path_matcher", "default_service"]
  }
}
