variable "prefix" {
  type = string
}
variable "env" {
  type = string
}
variable "rules" {
  type = map(map(object({
    action = string
    priority = number
    src_ip_ranges = list(string)
  })))
}
