class Reasondb < Formula
  desc "AI-native document database with hierarchical reasoning retrieval"
  homepage "https://github.com/reasondb/reasondb"
  license "ReasonDB-1.0"
  version "0.1.0"

  on_macos do
    on_arm do
      url "https://github.com/reasondb/reasondb/releases/download/v#{version}/reasondb-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_AARCH64_MACOS"
    end
    on_intel do
      url "https://github.com/reasondb/reasondb/releases/download/v#{version}/reasondb-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_X86_64_MACOS"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/reasondb/reasondb/releases/download/v#{version}/reasondb-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_AARCH64_LINUX"
    end
    on_intel do
      url "https://github.com/reasondb/reasondb/releases/download/v#{version}/reasondb-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_X86_64_LINUX"
    end
  end

  def install
    bin.install "reasondb"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/reasondb --version")
  end
end
