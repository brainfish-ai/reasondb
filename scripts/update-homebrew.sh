#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?Usage: update-homebrew.sh <version> <artifacts-dir>}"
ARTIFACTS_DIR="${2:?Usage: update-homebrew.sh <version> <artifacts-dir>}"

sha_for() {
  local file="${ARTIFACTS_DIR}/reasondb-${VERSION}-${1}.tar.gz"
  if [[ -f "$file" ]]; then
    sha256sum "$file" | awk '{print $1}'
  else
    echo ""
  fi
}

SHA_AARCH64_MACOS=$(sha_for "aarch64-apple-darwin")
SHA_X86_64_MACOS=$(sha_for "x86_64-apple-darwin")
SHA_AARCH64_LINUX=$(sha_for "aarch64-unknown-linux-gnu")
SHA_X86_64_LINUX=$(sha_for "x86_64-unknown-linux-gnu")

BARE_VERSION="${VERSION#v}"
REPO="brainfish-ai/reasondb"

# Build the on_macos block — only include architectures that were released
macos_block() {
  local block=""
  if [[ -n "$SHA_AARCH64_MACOS" ]]; then
    block+="    on_arm do\n"
    block+="      url \"https://github.com/${REPO}/releases/download/${VERSION}/reasondb-${VERSION}-aarch64-apple-darwin.tar.gz\"\n"
    block+="      sha256 \"${SHA_AARCH64_MACOS}\"\n"
    block+="    end\n"
  fi
  if [[ -n "$SHA_X86_64_MACOS" ]]; then
    block+="    on_intel do\n"
    block+="      url \"https://github.com/${REPO}/releases/download/${VERSION}/reasondb-${VERSION}-x86_64-apple-darwin.tar.gz\"\n"
    block+="      sha256 \"${SHA_X86_64_MACOS}\"\n"
    block+="    end\n"
  fi
  echo -e "$block"
}

linux_block() {
  local block=""
  if [[ -n "$SHA_AARCH64_LINUX" ]]; then
    block+="    on_arm do\n"
    block+="      url \"https://github.com/${REPO}/releases/download/${VERSION}/reasondb-${VERSION}-aarch64-unknown-linux-gnu.tar.gz\"\n"
    block+="      sha256 \"${SHA_AARCH64_LINUX}\"\n"
    block+="    end\n"
  fi
  if [[ -n "$SHA_X86_64_LINUX" ]]; then
    block+="    on_intel do\n"
    block+="      url \"https://github.com/${REPO}/releases/download/${VERSION}/reasondb-${VERSION}-x86_64-unknown-linux-gnu.tar.gz\"\n"
    block+="      sha256 \"${SHA_X86_64_LINUX}\"\n"
    block+="    end\n"
  fi
  echo -e "$block"
}

MACOS_BLOCK=$(macos_block)
LINUX_BLOCK=$(linux_block)

cat > Formula/reasondb.rb << FORMULA
class Reasondb < Formula
  desc "AI-native document database with hierarchical reasoning retrieval"
  homepage "https://github.com/${REPO}"
  license "ReasonDB-1.0"
  version "${BARE_VERSION}"

  on_macos do
${MACOS_BLOCK}
  end

  on_linux do
${LINUX_BLOCK}
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
[[ -n "$SHA_AARCH64_MACOS" ]] && echo "  macOS arm64:  ${SHA_AARCH64_MACOS}"
[[ -n "$SHA_X86_64_MACOS"  ]] && echo "  macOS x86_64: ${SHA_X86_64_MACOS}"
[[ -n "$SHA_AARCH64_LINUX" ]] && echo "  Linux arm64:  ${SHA_AARCH64_LINUX}"
[[ -n "$SHA_X86_64_LINUX"  ]] && echo "  Linux x86_64: ${SHA_X86_64_LINUX}"
