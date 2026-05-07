class Kasane < Formula
  desc "Alternative frontend for the Kakoune text editor"
  homepage "https://github.com/Yus314/kasane"
  version "0.6.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-aarch64-macos.tar.gz"
      sha256 "bf859f3844d68e62f7f54ea96619ce9ded228731f7aae302e52dcc0a20d57452"
    end
    on_intel do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-x86_64-macos.tar.gz"
      sha256 "5af62f590dec0d375c38dbd4e2b267c015f1d2b62c044434852e85c84121ee12"
    end
  end

  depends_on "kakoune"

  def install
    bin.install "kasane"
  end

  test do
    assert_match "kasane #{version}", shell_output("#{bin}/kasane --help")
  end
end
