class MtrNg < Formula
  desc "Modern My Traceroute with real-time network path visualization"
  homepage "https://github.com/edejong-dbc/mtr-ng"
  url "https://github.com/edejong-dbc/mtr-ng/archive/v0.3.0.tar.gz"
  # SHA256 will be calculated automatically by Homebrew
  license "MIT OR Apache-2.0"
  head "https://github.com/edejong-dbc/mtr-ng.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release"
    bin.install "target/release/mtr-ng"
    man1.install "install/mtr-ng.1"
  end

  test do
    # Test that the binary exists and shows help
    assert_match "Modern My Traceroute", shell_output("#{bin}/mtr-ng --help")
    
    # Test version output
    assert_match version.to_s, shell_output("#{bin}/mtr-ng --version")
  end
end 