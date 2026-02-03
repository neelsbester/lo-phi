class Lophi < Formula
  desc "Feature reduction using missing value, Gini/IV, and correlation analysis"
  homepage "https://github.com/neelsbester/lo-phi"
  version "1.0.0"
  license "MIT"

  on_macos do
    url "https://github.com/neelsbester/lo-phi/releases/download/v1.0.0/lophi-macos-aarch64.tar.gz"
    sha256 "PLACEHOLDER"
  end

  on_linux do
    url "https://github.com/neelsbester/lo-phi/releases/download/v1.0.0/lophi-linux-x86_64.tar.gz"
    sha256 "PLACEHOLDER"
  end

  def install
    bin.install "lophi"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/lophi --version")
  end
end
