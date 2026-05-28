class Kasane < Formula
  desc "Alternative frontend for the Kakoune text editor"
  homepage "https://github.com/Yus314/kasane"
  version "0.7.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-aarch64-macos.tar.gz"
      sha256 "fb646a2d8d8920caadb4dfe362e0262376bff80de17fda0cefd4fa9bd13ed6b0"
    end
    on_intel do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-x86_64-macos.tar.gz"
      sha256 "9cee9261d2cf3dbd08b569891aca751a8f3bffd1252c414b2feed9a5f1982902"
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
