class Kasane < Formula
  desc "Alternative frontend for the Kakoune text editor"
  homepage "https://github.com/Yus314/kasane"
  version "0.2.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-aarch64-macos.tar.gz"
      sha256 "7c5ae144922ef109709451f63e6d0dec2a5334a80da56a26f92939839836ae6c"
    end
    on_intel do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-x86_64-macos.tar.gz"
      sha256 "d69a150c0e71e3017c8d28a6cdd46c90be2c9a9598cf7e3c437fc05619c73e45"
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
