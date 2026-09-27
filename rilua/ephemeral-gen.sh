#!/usr/bin/env bash
set -euo pipefail

# Generate an ephemeral minisign keypair and encrypt the private key with AGE.
# The encrypted key can be passed between CI jobs via artifacts.
#
# Required environment:
#   AGE_KEY_PUBLIC - AGE public key for encrypting the minisign private key

if [[ -z "${AGE_KEY_PUBLIC:-}" ]]; then
    echo "Error: AGE_KEY_PUBLIC not set"
    exit 1
fi

echo "Installing required tools..."
cargo install rsign2 --quiet
cargo install rage --quiet

echo "Generating ephemeral minisign keypair..."
rsign generate -f -W -p minisign.pub -s minisign.key

# Mask the private key content in CI logs
key_content=$(tail -1 minisign.key)
masked_key="${key_content:0:32}[REDACTED]"
echo "::add-mask::${masked_key}"

# Log the public key ID
pub_key_id=$(head -1 minisign.pub | sed 's/.*: //')
echo "Generated ephemeral key with ID: ${pub_key_id}"

echo "Encrypting private key with AGE..."
rage -e -r "${AGE_KEY_PUBLIC}" minisign.key > minisign.key.age

# Remove unencrypted private key
rm -f minisign.key

echo "Ephemeral keypair generated and encrypted"
ls -la minisign.*
