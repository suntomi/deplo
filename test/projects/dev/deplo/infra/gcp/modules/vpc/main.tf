resource "null_resource" "dependency_getter" {
  provisioner "local-exec" {
    command = "echo ${length(var.dependencies)}"
  }
}

resource "google_compute_network" "vpc-network" {
  for_each = toset(var.envs)
  name = "${var.prefix}-${each.value}-vpc-network"  

  depends_on = [
    null_resource.dependency_getter
  ]
}
resource "google_compute_firewall" "default" {
  for_each = toset(var.envs)
  name    = "${var.prefix}-${each.value}-fw-allow-ssh"
  network = google_compute_network.vpc-network[each.value].name

  allow {
    protocol = "tcp"
    ports    = ["22"]
  }

  source_ranges = ["0.0.0.0/0"]
}

resource "google_compute_firewall" "health-check" {
  for_each = toset(var.envs)
  name    = "${var.prefix}-${each.value}-fw-allow-health-check"
  network = google_compute_network.vpc-network[each.value].name

  allow {
    protocol = "tcp"
    ports    = ["0-65535"]
  }

  // glb ip ranges
  source_ranges = ["35.191.0.0/16", "130.211.0.0/22"]
}

resource "google_compute_firewall" "allow-internal" {
  for_each = toset(var.envs)
  name    = "${var.prefix}-${each.value}-fw-allow-internal"
  network = google_compute_network.vpc-network[each.value].name
  description = "Allow internal traffic on the default network"

  allow {
    protocol = "tcp"
    ports    = ["0-65535"]
  }
  allow {
    protocol = "udp"
    ports    = ["0-65535"]
  }
  allow {
    protocol = "icmp"
  }
  # ip range which created automatically by google_compute_network
  source_ranges = ["10.128.0.0/9"]
}

resource "google_compute_global_address" "private_ip_range" {
  for_each      = toset(var.envs)
  name          = "${var.prefix}-${each.value}-private-ip-range"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = google_compute_network.vpc-network[each.value].name
}

resource "google_service_networking_connection" "private_vpc_connection" {
  for_each      = toset(var.envs)
  network       = google_compute_network.vpc-network[each.value].name
  service       = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.private_ip_range[each.value].name]

  depends_on = [
    null_resource.dependency_getter
  ]
}
