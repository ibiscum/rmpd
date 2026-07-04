#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

# Enable cross-compile support if configured.
_CC_ENV="$(dirname "$0")/scripts/cross-compile-env.sh"
if [ -f "$_CC_ENV" ]; then
    # shellcheck disable=SC1090
    . "$_CC_ENV"
else
    echo "Not using cross-compilation (${_CC_ENV} does not exist)"
fi

if [ -n "${DIST:-}" ]; then
    echo "Using distribution from DIST environment variable: $DIST"
    DIST_ARG="--dist=$DIST"
    DIST_NAME="$DIST"
else
    echo "No DIST environment variable set, using sbuild default"
    DIST_ARG=""
    DIST_NAME="$(dpkg-parsechangelog -S Distribution)"
fi

BACKPORTS_ARG="--extra-repository=deb http://deb.debian.org/debian ${DIST_NAME}-backports main"
BACKPORTS_CONF_CMD="printf 'APT::Default-Release \"${DIST_NAME}-backports\";\n' > /etc/apt/apt.conf.d/99default-release"

echo "Removing previous build target directories"
find . -type d -name target -prune -exec rm -rf {} +

if [ -d debian ]; then
    echo "Cleaning Debian build artifacts"
    if [ -x debian/rules ]; then
        debian/rules clean || true
    fi
    rm -rf debian/.debhelper \
           debian/rmpd \
           debian/rmpd-dbgsym \
           debian/files \
           debian/debhelper-build-stamp \
           debian/rmpd.debhelper.log \
           debian/rmpd.postrm.debhelper \
           debian/rmpd.substvars
fi

if command -v sbuild >/dev/null 2>&1; then
    sbuild --chroot-mode=unshare \
           --enable-network \
           "$BACKPORTS_ARG" \
           --chroot-setup-commands="$BACKPORTS_CONF_CMD" \
           --no-clean-source \
           $DIST_ARG
else
    echo "sbuild not found; falling back to local dpkg-buildpackage"
    dpkg-buildpackage -b -us -uc
fi