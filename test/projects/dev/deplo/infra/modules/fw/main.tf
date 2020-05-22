resource "google_compute_security_policy" "policy" {
  for_each = toset(keys(var.rules))
  name = "${var.project}-${var.env}-fw-rules-${each.value}"

  dynamic "rule" {
    for_each = toset(keys(var.rules[each.value]))
    content {
      action   = "${var.rules[each.value][rule.value].action}"
      priority = "${var.rules[each.value][rule.value].priority}"
      match {
        versioned_expr = "SRC_IPS_V1"
        config {
          src_ip_ranges = "${var.rules[each.value][rule.value].src_ip_ranges}"
        }
      }
      description = "rule for ${rule.value}"
    }
  }
}
