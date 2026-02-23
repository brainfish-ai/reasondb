#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?Usage: update-homebrew.sh <version> <checksums-dir>}"
CHECKSUMS_DIR="${2:?Usage: update-homebrew.sh <version> <checksums-dir>}"

sha_for() {
  local target="$1"
  grep "reasondb-${VERSION}-${target}" "${CHECKSUMS_DIR}/checksums-sha256.txt" | awk '{print $1}'
}

SHA_AARCH64_MACOS=$(sha_for "aarch64-apple-darwin")
SHA_X86_64_MACOS=$(sha_for "x86_64-apple-darwin")
SHA_AARCH64_LINUX=$(sha_for "aarch64-unknown-linux-gnu")
SHA_X86_64_LINUX=$(sha_for "x86_64-unknown-linux-gnu")

BARE_VERSION="${VERSION#v}"

cat > Formula/reasondb.rb << FORMULA
class Reasondb < Formula
  desc "AI-native document database with hierarchical reasoning retrieval"
  homepage "https://github.com/reasondb/reasondb"
  license "ReasonDB-1.0"
  version "${BARE_VERSION}"

  on_macos do
    on_arm do
      url "https://github.com/reasondb/reasondb/releases/download/${VERSION}/reasondb-${VERSION}-aarch64-apple-darwin.tar.gz"
      sha256 "${SHA_AARCH64_MACOS}"
    end
    on_intel do
      url "https://github.com/reasondb/reasondb/releases/download/${VERSION}/reasondb-${VERSION}-x86_64-apple-darwin.tar.gz"
      sha256 "${SHA_X86_64_MACOS}"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/reasondb/reasondb/releases/download/${VERSION}/reasondb-${VERSION}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "${SHA_AARCH64_LINUX}"
    end
    on_intel do
      url "https://github.com/reasondb/reasondb/releases/download/${VERSION}/reasondb-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "${SHA_X86_64_LINUX}"
    end
  end

  def install
    bin.install "reasondb"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/reasondb --version")
  end
end
FORMULA

echo "Formula updated for ${VERSION}"
echo "  macOS arm64:  ${SHA_AARCH64_MACOS}"
echo "  macOS x86_64: ${SHA_X86_64_MACOS}"
echo "  Linux arm64:  ${SHA_AARCH64_LINUX}"
echo "  Linux x86_64: ${SHA_X86_64_LINUX}"
