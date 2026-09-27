#!/usr/bin/env bash
set -euo pipefail

# Decrypt the ephemeral minisign key and sign files.
# Signatures include trusted comments with repository and build metadata.
#
# Required environment:
#   AGE_KEY_SECRET - AGE secret key for decrypting the minisign private key
#
# Usage: ./ephemeral-sign.sh <file1> [file2] ...

if [[ -z "${AGE_KEY_SECRET:-}" ]]; then
    echo "Error: AGE_KEY_SECRET not set"
    exit 1
fi

if [[ -z "${GITHUB_REPOSITORY:-}" ]]; then
    GITHUB_REPOSITORY="unknown"
fi

if [[ -z "${GITHUB_RUN_ID:-}" ]]; then
    GITHUB_RUN_ID="unknown"
fi

# Create temporary AGE key file
age_key_file=$(mktemp age.key.XXXXXXXXXX)
trap 'rm -f "${age_key_file}" minisign.key' EXIT

echo "${AGE_KEY_SECRET}" > "${age_key_file}"

echo "Decrypting minisign private key..."
rage -d -i "${age_key_file}" minisign.key.age > minisign.key

# Build trusted comment with metadata
timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")
git_commit=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
comment="gh=${GITHUB_REPOSITORY} git=${git_commit} ts=${timestamp} run=${GITHUB_RUN_ID}"

echo "Signing with metadata: ${comment}"

for file in "$@"; do
    if [[ -f "$file" ]]; then
        echo "Signing ${file}..."
        rsign sign -W -s minisign.key -t "${comment}" "${file}"

        # Rename .minisig to .sig for consistency
        if [[ -f "${file}.minisig" ]]; then
            mv "${file}.minisig" "${file}.sig"
        fi

        echo "Created signature: ${file}.sig"
    else
        echo "Warning: File not found: ${file}"
    fi
done

echo "Signing complete"
