#!/usr/bin/env bash
# Build tempmail, import into k3s' containerd, apply manifests.
# Re-run any time the backend code or manifests change.
#
# Bootstrap order (one-time, before the first deploy):
#   1. DNS A records: temp-mail.jalencode.com and mail.jalencode.com → public IP.
#      MX for jalencode.com → 10 mail.jalencode.com.
#   2. Router: forward TCP/25, TCP/80, TCP/443 to cluster1's LAN IP.
#   3. sudo nixos-rebuild switch --flake .#cluster1
#      certbot-issue.service auto-issues the two certs; tempmail-cert-sync
#      mirrors mail.jalencode.com's cert into /var/lib/tempmail/tls/.
#   4. Run this script.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE_TAG="tempmail:0.1.0"
CERT_DIR="/var/lib/tempmail/tls"

echo "🔎 Cert preflight..."
# Need sudo because tempmail-host.nix locks $CERT_DIR to mode 0750 uid 1000,
# and the operator running this script is usually not uid 1000.
if ! sudo test -f "$CERT_DIR/fullchain.pem" || ! sudo test -f "$CERT_DIR/key.pem"; then
  cat >&2 <<EOF
❌ Missing $CERT_DIR/{fullchain.pem,key.pem}.

The tempmail SMTP listener will refuse to start without these because
[smtp].tls_cert / tls_key are set in the Secret (config.toml).

Fix:
  sudo certbot certonly --webroot -w /var/lib/acme/acme-challenge -d mail.jalencode.com
  sudo systemctl start tempmail-cert-sync

Or, to deploy receive-only without STARTTLS, comment out tls_cert/tls_key
in manifests/tempmail-secret.yaml and re-run.
EOF
  exit 1
fi
echo "   ✓ certs present"

echo "🔨 Building tempmail image ($IMAGE_TAG)..."
docker build -t "$IMAGE_TAG" "$REPO_ROOT/backend"

echo "📦 Importing image into k3s containerd..."
TMP_TAR="$(mktemp --suffix=.tar)"
trap 'rm -f "$TMP_TAR"' EXIT
docker save "$IMAGE_TAG" -o "$TMP_TAR"
sudo k3s ctr images import "$TMP_TAR"

echo "🚀 Applying manifests..."
sudo kubectl apply -f "$REPO_ROOT/manifests/tempmail-final.yaml"
sudo kubectl apply -f "$REPO_ROOT/manifests/tempmail-secret.yaml"

echo "♻️  Rolling the Deployment..."
sudo kubectl -n tempmail rollout restart deployment/tempmail
sudo kubectl -n tempmail rollout status deployment/tempmail --timeout=180s

echo "✅ Status:"
sudo kubectl -n tempmail get pods,svc -o wide

cat <<'NEXT'

Smoke tests:
  # HTTP API (auth check):
  curl -sS -H "Authorization: Bearer <api_token from Secret>" \
    https://temp-mail.domain.com/v1/addresses

  # SMTP banner:
  echo "QUIT" | nc -v mail.domain.com 25

  # STARTTLS:
  openssl s_client -connect mail.domain.com:25 -starttls smtp -servername mail.domain.com

  # End-to-end (from another host with mail):
  swaks --to test@domain.com --server mail.domain.com:25
NEXT
