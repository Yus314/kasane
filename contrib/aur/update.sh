#!/usr/bin/env bash
# Regenerate PKGBUILD and .SRCINFO for kasane-bin AUR package.
# Called by release.yml after binaries are built.
#
# Usage: update.sh VERSION X86_64_HASH AARCH64_HASH MIT_HASH APACHE_HASH
set -euo pipefail

[[ $# -eq 5 ]] || { echo "Usage: $0 VERSION X86_64_HASH AARCH64_HASH MIT_HASH APACHE_HASH" >&2; exit 1; }

VERSION="$1"
X86_HASH="$2"
AARCH64_HASH="$3"
MIT_HASH="$4"
APACHE_HASH="$5"

DIR="$(cd "$(dirname "$0")/kasane-bin" && pwd)"

# --- PKGBUILD (quoted heredoc preserves ${pkgver} etc.) ---
cat > "$DIR/PKGBUILD" << 'PKGBUILD_TEMPLATE'
# Maintainer: Yus314 <https://github.com/Yus314>
pkgname=kasane-bin
pkgver=__PKGVER__
pkgrel=1
pkgdesc='Alternative frontend for the Kakoune text editor (prebuilt binary)'
arch=('x86_64' 'aarch64')
url='https://github.com/Yus314/kasane'
license=('MIT' 'Apache-2.0')
depends=('kakoune>=2024.12.09' 'vulkan-icd-loader' 'wayland' 'libxkbcommon')
provides=('kasane')
conflicts=('kasane')

source=("LICENSE-MIT::https://raw.githubusercontent.com/Yus314/kasane/v${pkgver}/LICENSE-MIT"
        "LICENSE-APACHE::https://raw.githubusercontent.com/Yus314/kasane/v${pkgver}/LICENSE-APACHE")
sha256sums=('__MIT_HASH__'
            '__APACHE_HASH__')

source_x86_64=("kasane-v${pkgver}-x86_64-linux-gnu.tar.gz::https://github.com/Yus314/kasane/releases/download/v${pkgver}/kasane-v${pkgver}-x86_64-linux-gnu.tar.gz")
sha256sums_x86_64=('__X86_HASH__')

source_aarch64=("kasane-v${pkgver}-aarch64-linux-gnu.tar.gz::https://github.com/Yus314/kasane/releases/download/v${pkgver}/kasane-v${pkgver}-aarch64-linux-gnu.tar.gz")
sha256sums_aarch64=('__AARCH64_HASH__')

package() {
    install -Dm755 kasane "${pkgdir}/usr/bin/kasane"
    install -Dm644 LICENSE-MIT "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE-MIT"
    install -Dm644 LICENSE-APACHE "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE-APACHE"
}
PKGBUILD_TEMPLATE

sed -i \
  -e "s/__PKGVER__/${VERSION}/g" \
  -e "s/__MIT_HASH__/${MIT_HASH}/g" \
  -e "s/__APACHE_HASH__/${APACHE_HASH}/g" \
  -e "s/__X86_HASH__/${X86_HASH}/g" \
  -e "s/__AARCH64_HASH__/${AARCH64_HASH}/g" \
  "$DIR/PKGBUILD"

# --- .SRCINFO (expanded values, no ${pkgver}) ---
cat > "$DIR/.SRCINFO" << SRCINFO_TEMPLATE
pkgbase = kasane-bin
	pkgdesc = Alternative frontend for the Kakoune text editor (prebuilt binary)
	pkgver = ${VERSION}
	pkgrel = 1
	url = https://github.com/Yus314/kasane
	arch = x86_64
	arch = aarch64
	license = MIT
	license = Apache-2.0
	depends = kakoune>=2024.12.09
	depends = vulkan-icd-loader
	depends = wayland
	depends = libxkbcommon
	provides = kasane
	conflicts = kasane
	source = LICENSE-MIT::https://raw.githubusercontent.com/Yus314/kasane/v${VERSION}/LICENSE-MIT
	source = LICENSE-APACHE::https://raw.githubusercontent.com/Yus314/kasane/v${VERSION}/LICENSE-APACHE
	sha256sums = ${MIT_HASH}
	sha256sums = ${APACHE_HASH}
	source_x86_64 = kasane-v${VERSION}-x86_64-linux-gnu.tar.gz::https://github.com/Yus314/kasane/releases/download/v${VERSION}/kasane-v${VERSION}-x86_64-linux-gnu.tar.gz
	sha256sums_x86_64 = ${X86_HASH}
	source_aarch64 = kasane-v${VERSION}-aarch64-linux-gnu.tar.gz::https://github.com/Yus314/kasane/releases/download/v${VERSION}/kasane-v${VERSION}-aarch64-linux-gnu.tar.gz
	sha256sums_aarch64 = ${AARCH64_HASH}

pkgname = kasane-bin
SRCINFO_TEMPLATE

# --- docs/getting-started.md version ---
REPO_ROOT="$(cd "$DIR/../../.." && pwd)"
sed -i "s/^VERSION=.*/VERSION=${VERSION}/" "$REPO_ROOT/docs/getting-started.md"

echo "Updated PKGBUILD, .SRCINFO, and getting-started.md for kasane-bin ${VERSION}"
