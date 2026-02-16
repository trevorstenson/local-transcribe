#!/usr/bin/env bash
set -euo pipefail

# Build a release of Wren.
# Tauri automatically signs and notarizes when the right env vars are set.
#
# For code signing, export:
#   APPLE_SIGNING_IDENTITY  - e.g. "Developer ID Application: Name (TEAMID)"
#   APPLE_CERTIFICATE       - base64-encoded .p12 certificate
#   APPLE_CERTIFICATE_PASSWORD
#
# For notarization, also export:
#   APPLE_ID       - your Apple ID email
#   APPLE_PASSWORD - app-specific password (not your account password)
#   APPLE_TEAM_ID  - your 10-character team ID

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  echo "⚠  APPLE_SIGNING_IDENTITY is not set — the build will NOT be code-signed."
  echo "   Users who download this app will need to run:"
  echo "     xattr -cr /Applications/Wren.app"
  echo ""
fi

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ] && [ -z "${APPLE_ID:-}" ]; then
  echo "⚠  Signing is configured but notarization vars are missing (APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID)."
  echo "   The build will be signed but NOT notarized — Gatekeeper may still block it."
  echo ""
fi

echo "Building Wren release..."
npm run tauri build

echo ""
echo "Done. Artifacts are in src-tauri/target/release/bundle/"
