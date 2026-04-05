class Kasane < Formula
  desc "Alternative frontend for the Kakoune text editor"
  homepage "https://github.com/Yus314/kasane"
  version "0.3.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-aarch64-macos.tar.gz"
      sha256 "ed48eadbeb41385761a6370b2d5803eb437403985285ee11b79677c483d03ffc"
    end
    on_intel do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-x86_64-macos.tar.gz"
      sha256 "1fe7f25f615d5f3c35346f13d716e70a1d094f42bfb715433ea7400966af01f7"
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
