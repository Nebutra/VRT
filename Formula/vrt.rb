# Homebrew formula for VRT (agent-native local verification runtime).
#
# This repo is itself the tap. Install with:
#
#   brew tap nebutra/vrt https://github.com/Nebutra/VRT
#   brew install vrt
#
# Until the first tagged release exists, install the latest main:
#
#   brew install --HEAD nebutra/vrt/vrt
#
# The `url`/`sha256`/`version` below are maintained automatically on each tag by
# scripts/bump-homebrew-formula.mjs (run from the Release workflow).
class Vrt < Formula
  desc "Agent-native local verification runtime for fast, auditable code checks"
  homepage "https://github.com/Nebutra/VRT"
  url "https://github.com/Nebutra/VRT/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "a41be3cadea202391231cc2facddca8275ce1aad1a70d59c1929e68bd76b7f30"
  license "MIT"
  head "https://github.com/Nebutra/VRT.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/vrt-cli")
  end

  test do
    assert_match "vrt", shell_output("#{bin}/vrt --version")
    # `vrt init` understands a project and writes the .vrt workspace.
    (testpath/"package.json").write <<~JSON
      { "name": "brew-test", "private": true, "scripts": { "typecheck": "true" } }
    JSON
    system "git", "-c", "init.defaultBranch=main", "init", testpath
    output = shell_output("#{bin}/vrt --root #{testpath} doctor")
    assert_match "Verification capabilities", output
  end
end
