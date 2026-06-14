# Homebrew formula for Tessera. Lives in this repo as the canonical source;
# copy it into the tap repo `iamsaquib8/homebrew-tessera` (path: Formula/tessera.rb)
# so users can `brew install iamsaquib8/tessera/tessera`.
#
# Builds from source — no per-arch bottle SHAs to maintain. On each release,
# bump `url` to the new tag and update `sha256` (the tarball's sha256), e.g.:
#   curl -sL https://github.com/iamsaquib8/tessera/archive/refs/tags/vX.Y.Z.tar.gz | shasum -a 256
# or run `brew bump-formula-pr` to automate it.
class Tessera < Formula
  desc "Local, deterministic semantic code graph + MCP server for AI coding agents"
  homepage "https://github.com/iamsaquib8/tessera"
  url "https://github.com/iamsaquib8/tessera/archive/refs/tags/v0.4.0.tar.gz"
  sha256 "REPLACE_WITH_TARBALL_SHA256"
  license "Apache-2.0"
  head "https://github.com/iamsaquib8/tessera.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "tessera", shell_output("#{bin}/tessera --version")
  end
end
