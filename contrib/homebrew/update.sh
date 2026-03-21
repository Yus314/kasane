#!/usr/bin/env bash
# Regenerate Homebrew formula for kasane.
# Called by release.yml after macOS binaries are built.
#
# Usage: update.sh VERSION X86_64_MACOS_HASH AARCH64_MACOS_HASH
set -euo pipefail

[[ $# -eq 3 ]] || { echo "Usage: $0 VERSION X86_64_MACOS_HASH AARCH64_MACOS_HASH" >&2; exit 1; }

VERSION="$1"
X86_HASH="$2"
AARCH64_HASH="$3"

DIR="$(cd "$(dirname "$0")" && pwd)"

cat > "$DIR/kasane.rb" << 'FORMULA_TEMPLATE'
class Kasane < Formula
  desc "Alternative frontend for the Kakoune text editor"
  homepage "https://github.com/Yus314/kasane"
  version "__VERSION__"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-aarch64-macos.tar.gz"
      sha256 "__AARCH64_HASH__"
    end
    on_intel do
      url "https://github.com/Yus314/kasane/releases/download/v#{version}/kasane-v#{version}-x86_64-macos.tar.gz"
      sha256 "__X86_HASH__"
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
FORMULA_TEMPLATE

sed -i \
  -e "s/__VERSION__/${VERSION}/g" \
  -e "s/__X86_HASH__/${X86_HASH}/g" \
  -e "s/__AARCH64_HASH__/${AARCH64_HASH}/g" \
  "$DIR/kasane.rb"

echo "Updated Homebrew formula for kasane ${VERSION}"
