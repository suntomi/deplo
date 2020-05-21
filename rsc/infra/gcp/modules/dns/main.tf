resource "google_dns_managed_zone" "root" {
  count = "${length(var.predefined_zone) > 0 ? 0 : 1}"
  name = "${var.project}-zone"
  dns_name = "${var.root_domain}."
  description = "zone:${var.project}:${var.root_domain}"
}
