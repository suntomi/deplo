resource "google_compute_network" "vpc-network" {
  name = "${var.project}-${var.env}-vpc-network"  
}
resource "google_compute_firewall" "default" {
  // TODO: 最終的にはprodのやつは消す. 
  name    = "${var.project}-${var.env}-fw-allow-ssh"
  network = "${google_compute_network.vpc-network.name}"

  allow {
    protocol = "tcp"
    ports    = ["22"]
  }

  source_ranges = ["0.0.0.0/0"]
}

resource "google_compute_firewall" "health-check" {
  name    = "${var.project}-${var.env}-fw-allow-health-check"
  network = "${google_compute_network.vpc-network.name}"

  allow {
    protocol = "tcp"
    ports    = "${concat(["80", "443"], var.extra_service_ports)}"
  }

  // glb ip ranges
  source_ranges = ["35.191.0.0/16", "130.211.0.0/22"]
}

resource "google_compute_firewall" "allow-internal" {
  name    = "${var.project}-${var.env}-fw-allow-internal"
  network = "${google_compute_network.vpc-network.name}"
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
  source_ranges = ["10.128.0.0/9"] # google_compute_networkで自動作成されるIP範囲
}

resource "google_compute_global_address" "sql_db_private_ip_range" {
  provider = "google-beta"

  name          = "${var.project}-${var.env}-sql-db-private-ip-range"
  purpose       = "VPC_PEERING"
  address_type = "INTERNAL"
  prefix_length = 16
  network       = "${google_compute_network.vpc-network.name}"
}

resource "google_service_networking_connection" "sql_db_private_vpc_connection" {
  network       = "${google_compute_network.vpc-network.name}"
  service       = "servicenetworking.googleapis.com"
  reserved_peering_ranges = ["${google_compute_global_address.sql_db_private_ip_range.name}"]
}


// official resources are just added a week ago: 
// https://github.com/terraform-providers/terraform-provider-google/issues/3401
// work around: we do this in deploy scripts
/*
resource "google_vpc_access_connector" "connector" {
  provider      = "google-beta"
  name          = "%s"
  region        = "us-central1"
  ip_cidr_range = "10.10.0.0/28"
  network       = "default"
}
*/