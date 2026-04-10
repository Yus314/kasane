class Kasane < Formula
  desc "Alternative frontend for the Kakoune text editor"
  homepage "https://github.com/Yus314/kasane"
  version "0.5.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-aarch64-macos.tar.gz"
      sha256 "9f8903cab3566b085182f1012143cb370c9a5c51d44ad33d6eeda501d7e9af13"
    end
    on_intel do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-x86_64-macos.tar.gz"
      sha256 "955cc41557a5ff8e2871b1a93efeff171c77851cc69236d00e1b6725bb530e95"
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
