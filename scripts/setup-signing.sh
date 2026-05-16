#!/usr/bin/env bash
# scripts/setup-signing.sh
#
# One-time setup: create a self-signed code-signing certificate, import it
# into the login Keychain, and trust it for code signing. Tauri's
# `bundle.macOS.signingIdentity` setting picks it up by Common Name.
#
# Why this matters: an ad-hoc signed .app gets a new codesign hash on every
# rebuild, which means macOS TCC (Screen Recording, Microphone) treats every
# rebuild as a brand-new app. Grants don't persist. A stable self-signed
# certificate fixes that — same codesign hash forever, same TCC identity.
#
# Idempotent: safe to re-run; no-op if the cert already exists.

set -euo pipefail

CERT_NAME="${CERT_NAME:-Lord Varys Self-Signed}"
KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"
DAYS_VALID="${DAYS_VALID:-3650}"  # 10 years

# Already there? Bail early.
if security find-identity -v -p codesigning "$KEYCHAIN" | grep -q "$CERT_NAME"; then
    echo "✓ '$CERT_NAME' already in your login keychain — nothing to do."
    security find-identity -v -p codesigning "$KEYCHAIN" | grep "$CERT_NAME"
    exit 0
fi

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

cd "$TMPDIR"

echo "→ Generating private key…"
openssl genrsa -out lordvarys.key 2048 2>/dev/null

# LibreSSL on macOS doesn't accept `-key` + `-x509` together for some
# reason, so we use the two-step CSR + self-sign path. Works on LibreSSL
# 3.x and OpenSSL 1.1+.
echo "→ Generating CSR…"
openssl req -new -nodes \
    -key lordvarys.key \
    -out lordvarys.csr \
    -subj "/CN=${CERT_NAME}"

cat > x509.ext <<EOF
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
basicConstraints = critical, CA:false
EOF

echo "→ Self-signing certificate…"
openssl x509 -req -days "$DAYS_VALID" \
    -in lordvarys.csr \
    -signkey lordvarys.key \
    -extfile x509.ext \
    -out lordvarys.crt 2>&1 | grep -v "Getting CA" | grep -v "signature ok" || true

# Bundle key + cert into a .p12 (Keychain's preferred import format).
echo "→ Bundling into PKCS#12 archive…"
P12_PASS="$(uuidgen)"  # transient; never persisted
openssl pkcs12 -export \
    -inkey lordvarys.key \
    -in lordvarys.crt \
    -name "$CERT_NAME" \
    -out lordvarys.p12 \
    -password "pass:${P12_PASS}"

echo "→ Importing into login keychain…"
security import lordvarys.p12 \
    -k "$KEYCHAIN" \
    -P "$P12_PASS" \
    -T /usr/bin/codesign \
    -T /usr/bin/security \
    -A

echo "→ Trusting cert for code signing…"
# This prompts for your account password — needed to add to the trust store.
security add-trusted-cert \
    -d \
    -r trustRoot \
    -p codeSign \
    -k "$KEYCHAIN" \
    lordvarys.crt

echo
echo "✓ Done. Verifying…"
security find-identity -v -p codesigning "$KEYCHAIN" | grep "$CERT_NAME"

echo
echo "Next: build with the cert by setting tauri.conf.json's"
echo "  bundle.macOS.signingIdentity to '$CERT_NAME'"
echo "(or export APPLE_SIGNING_IDENTITY='$CERT_NAME' before pnpm tauri:build)."
