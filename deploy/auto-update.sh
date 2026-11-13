#!/bin/bash
# Apply minor and patch releases of lab-resource-manager without a person.
#
# Run by lab-resource-manager-update.timer. A release is applied only when
# its major version matches the installed one, so breaking changes always
# wait for an administrator who has read the migration guide.
#
# What has to hold before the binary is replaced:
#   - the release is published (not a draft or a pre-release)
#   - the archive's sha256 matches the digest GitHub reports for the asset
#   - the archive's systemd unit is identical to the installed one
#     (a changed unit needs install.sh, so the update is left to a person)
#
# After the restart the service must stay active with the same main PID for
# HEALTH_WAIT_SECS. Otherwise the previous binary is put back, the service is
# restarted on it, and this script exits non-zero so the unit shows as failed.
set -euo pipefail

REPO="${LRM_UPDATE_REPO:-kano-lab/lab-resource-manager}"
TARGET="${LRM_UPDATE_TARGET:-x86_64-unknown-linux-musl}"
SERVICE="lab-resource-manager"
BIN="/usr/local/bin/lab-resource-manager"
UNIT_FILE="/etc/systemd/system/lab-resource-manager.service"
STATE_DIR="/var/lib/lab-resource-manager-updater"
VERSION_FILE="$STATE_DIR/installed-version"
HEALTH_WAIT_SECS="${LRM_UPDATE_HEALTH_WAIT_SECS:-60}"
SEMVER_RE='^[0-9]+\.[0-9]+\.[0-9]+$'
WORKDIR=""

log() { echo "auto-update: $*"; }
die() { echo "auto-update: $*" >&2; exit 1; }

read_installed_version() {
    [[ -f "$VERSION_FILE" ]] || die "$VERSION_FILE is missing; write the installed version (e.g. 3.0.0) into it"
    local version
    version="$(tr -d '[:space:]' < "$VERSION_FILE")"
    [[ "$version" =~ $SEMVER_RE ]] || die "$VERSION_FILE holds '$version', not X.Y.Z"
    echo "$version"
}

fetch_releases() {
    curl -fsSL --retry 3 --retry-delay 10 \
        -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/$REPO/releases?per_page=100"
}

# Published X.Y.Z versions, one per line, oldest first.
published_versions() {
    jq -r '.[] | select(.draft == false and .prerelease == false) | .tag_name' <<< "$1" \
        | sed -n 's/^v//p' | { grep -E "$SEMVER_RE" || true; } | sort -V
}

# Prints the newest published X.Y.Z whose major equals $2, or nothing.
newest_in_major() {
    published_versions "$1" | awk -F. -v m="$2" '$1 == m' | tail -n 1
}

newest_overall() { published_versions "$1" | tail -n 1; }

is_newer() { [[ "$1" != "$2" && "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n 1)" == "$1" ]]; }

# Downloads and verifies the archive for v$2 into $3, extracting it there.
download_verified() {
    local releases="$1" version="$2" workdir="$3"
    local name="lab-resource-manager-$TARGET.tar.gz" asset url digest actual
    asset="$(jq -c --arg tag "v$version" --arg name "$name" \
        '.[] | select(.tag_name == $tag) | .assets[] | select(.name == $name)' <<< "$releases")"
    [[ -n "$asset" ]] || die "v$version has no asset $name"
    url="$(jq -r '.browser_download_url' <<< "$asset")"
    digest="$(jq -r '.digest // empty' <<< "$asset")"
    [[ "$digest" == sha256:* ]] || die "v$version: GitHub reports no sha256 digest for $name"

    curl -fsSL --retry 3 --retry-delay 10 -o "$workdir/$name" "$url"
    actual="sha256:$(sha256sum "$workdir/$name" | cut -d' ' -f1)"
    [[ "$actual" == "$digest" ]] || die "v$version: digest mismatch (expected $digest, got $actual)"

    tar -xzf "$workdir/$name" -C "$workdir"
    [[ -x "$workdir/lab-resource-manager" ]] || die "v$version: archive has no lab-resource-manager binary"
}

unit_unchanged() {
    local workdir="$1"
    cmp -s "$workdir/deploy/lab-resource-manager.service" "$UNIT_FILE"
}

main_pid() { systemctl show -p MainPID --value "$SERVICE"; }

service_stays_up() {
    local pid
    sleep 5
    pid="$(main_pid)"
    [[ "$pid" != "0" ]] || return 1
    sleep "$HEALTH_WAIT_SECS"
    systemctl is-active --quiet "$SERVICE" && [[ "$(main_pid)" == "$pid" ]]
}

swap_and_check() {
    local workdir="$1" version="$2" previous="$BIN.prev"
    cp -p "$BIN" "$previous"
    install -m 755 -o root -g root "$workdir/lab-resource-manager" "$BIN"
    systemctl restart "$SERVICE"
    if service_stays_up; then
        echo "$version" > "$VERSION_FILE"
        log "now running v$version (previous binary kept at $previous)"
        return 0
    fi
    log "v$version did not stay up; restoring the previous binary"
    install -m 755 -o root -g root "$previous" "$BIN"
    systemctl restart "$SERVICE"
    die "rolled back after v$version failed its health check"
}

main() {
    [[ $EUID -eq 0 ]] || die "must run as root"
    mkdir -p "$STATE_DIR"
    exec 9> "$STATE_DIR/lock"
    flock -n 9 || die "another update is running"

    local installed releases candidate latest
    installed="$(read_installed_version)"
    releases="$(fetch_releases)"
    candidate="$(newest_in_major "$releases" "${installed%%.*}")"
    latest="$(newest_overall "$releases")"

    if [[ -n "$latest" && "${latest%%.*}" != "${installed%%.*}" ]] && is_newer "$latest" "$installed"; then
        log "v$latest is a new major version; it is not applied automatically (see docs/MIGRATION.md)"
    fi
    if [[ -z "$candidate" ]] || ! is_newer "$candidate" "$installed"; then
        log "v$installed is the newest release in its major version"
        return 0
    fi

    log "updating v$installed -> v$candidate"
    WORKDIR="$(mktemp -d)"
    trap 'rm -rf "$WORKDIR"' EXIT
    download_verified "$releases" "$candidate" "$WORKDIR"
    if ! unit_unchanged "$WORKDIR"; then
        die "v$candidate changes the systemd unit; install it by hand with deploy/install.sh"
    fi
    swap_and_check "$WORKDIR" "$candidate"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
