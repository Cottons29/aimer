#!/usr/bin/env bash
# Generate a self-signed TLS certificate for the Trunk dev server.
#
# WebGPU (navigator.gpu / GPUCanvasContext) is only exposed in a browser
# "secure context": https:// pages, or the hard-coded-trusted localhost /
# 127.0.0.1. Serving over plain http on a LAN IP (e.g. http://192.168.0.100:3000)
# is an *insecure* context, so the browser disables WebGPU and wgpu panics with
# "canvas context is not a GPUCanvasContext".
#
# Serving over https makes the LAN IP a secure context so WebGPU works. Browsers
# validate the certificate against the address in the URL bar, and for a bare IP
# that address must appear in the certificate's Subject Alternative Name (SAN) as
# an IP entry (CN is ignored for matching). This script therefore auto-detects
# the machine's LAN IPv4 addresses and lets you pass extra hosts/IPs.
#
# Usage:
#   ./gen_certs.sh                 # localhost, 127.0.0.1 + auto-detected LAN IPs
#   ./gen_certs.sh 192.168.0.100   # also pin an explicit IP/host
#
# Output: self_signed_certs/{key.pem,cert.pem} next to this script.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="${SCRIPT_DIR}/self_signed_certs"
mkdir -p "${OUT_DIR}"

# Base SAN entries that always apply.
dns_names=("localhost")
ip_addrs=("127.0.0.1" "::1")

# Auto-detect LAN IPv4 addresses (macOS + Linux friendly).
if command -v ipconfig >/dev/null 2>&1; then
  for iface in $(ifconfig -l 2>/dev/null); do
    ip="$(ipconfig getifaddr "${iface}" 2>/dev/null || true)"
    [ -n "${ip}" ] && ip_addrs+=("${ip}")
  done
elif command -v hostname >/dev/null 2>&1; then
  for ip in $(hostname -I 2>/dev/null); do
    ip_addrs+=("${ip}")
  done
fi

# Extra hosts/IPs passed on the command line.
for arg in "$@"; do
  if [[ "${arg}" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    ip_addrs+=("${arg}")
  else
    dns_names+=("${arg}")
  fi
done

# Build the [alt_names] section, de-duplicating entries.
# (macOS ships bash 3.2, which lacks associative arrays, so track "seen"
# entries in a plain newline-delimited string.)
alt_names=""
seen=$'\n'
i=1
for name in "${dns_names[@]}"; do
  case "${seen}" in *$'\n'"dns:${name}"$'\n'*) continue ;; esac
  seen="${seen}dns:${name}"$'\n'
  alt_names+="DNS.${i} = ${name}"$'\n'
  i=$((i + 1))
done
i=1
for ip in "${ip_addrs[@]}"; do
  case "${seen}" in *$'\n'"ip:${ip}"$'\n'*) continue ;; esac
  seen="${seen}ip:${ip}"$'\n'
  alt_names+="IP.${i} = ${ip}"$'\n'
  i=$((i + 1))
done

CONFIG="$(mktemp)"
trap 'rm -f "${CONFIG}"' EXIT
cat >"${CONFIG}" <<EOF
[req]
distinguished_name = dn
x509_extensions = v3_ext
prompt = no

[dn]
CN = aimer-dev

[v3_ext]
subjectAltName = @alt_names
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth

[alt_names]
${alt_names}
EOF

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "${OUT_DIR}/key.pem" \
  -out "${OUT_DIR}/cert.pem" \
  -days 825 \
  -config "${CONFIG}"

echo "Generated self-signed certificate:"
echo "  ${OUT_DIR}/key.pem"
echo "  ${OUT_DIR}/cert.pem"
echo "SAN entries:"
echo "${alt_names}" | sed 's/^/  /'
