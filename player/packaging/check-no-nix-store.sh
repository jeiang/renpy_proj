#!/usr/bin/env bash
# Fails when a file in a package names a Nix store path that the program can load code from (the bytes of the
# files, not only the load commands). It prints every other store path with the file that names it and the
# decision for it.
#   check-no-nix-store.sh <package-dir>
# A static library can hardcode a store path in a string (the nixpkgs macOS libffi did, for the dylib that
# holds its closure trampolines). The path does not exist on a user's machine, so the feature breaks there.
#
# Two classes of hit:
#   - a path that ends in .dylib, .so, .a or .dll: a library that can be loaded. This fails the check.
#     NIX_STORE_ALLOW is an extended regular expression for "<file name> <path>" pairs that are known and
#     accepted.
#   - any other path: a default config, locale, include or data folder. A missing folder is skipped by the
#     libraries that name it, so the path is listed and counted, and does not fail the check. Each one is
#     judged for what it does on a Mac without Nix. The decision is in DATA_DECISIONS below. A path that no
#     entry matches is printed as "UNREVIEWED": read it and add an entry (or fix the cause) when it shows up.
# Text in the licenses folder is not checked.
#
# Decisions, by class (each line of DATA_DECISIONS is "<regex for '<file name> <path>'> <TAB> <decision>"):
#   - OpenSSL default cert file and folder (the player binary and libcrypto.3.dylib): FIXED, not accepted.
#     A clean Mac has no such file, so Python `ssl` and the OpenSSL in FFmpeg's dependencies would verify no
#     certificate. The player sets SSL_CERT_FILE=/etc/ssl/cert.pem (the macOS root bundle) at start when the
#     user set no SSL_CERT_FILE or SSL_CERT_DIR (crates/player/src/main.rs, default_ca_bundle). packaging/macos.sh
#     proves it with packaging/ssl_probe.rpy, run from the bundled interpreter. The folder strings that stay
#     (certs, private, engines, ossl-modules, ct_log_list.cnf, openssl.cnf) are optional: OpenSSL skips a
#     missing folder or config file, and the default provider is built in.
#   - Python prefix, sys.path, zoneinfo: no hit. pyhost starts an isolated interpreter with an empty sys.path
#     and no `site` (crates/pyhost/src/lib.rs), and CPython's own prefix strings are not in the binary.
#     packaging/python_probe.rpy proves that sys.prefix, sys.path and zoneinfo.TZPATH name no store path.
#   - FFmpeg (libav*, libsw*): build configuration strings (prefix and output folders of the Nix build,
#     printed by avutil_configuration). FFmpeg reads no file from them. Accepted.
#   - GnuTLS config folder, gettext/GnuTLS/zvbi locale folders: a missing config is skipped, a missing locale
#     folder gives English messages. Accepted. (GnuTLS trusts /etc/ssl/certs/ca-certificates.crt, which a Mac
#     does not have, so https through FFmpeg's GnuTLS has no roots. The player opens local files only.)
#   - p11-kit module folders: a missing folder means no PKCS#11 module is found. Accepted.
#   - fontconfig built-in config (dejavu fonts folder, conf.avail): the FFmpeg dependency chain (libass) uses
#     it, the player renders text with FreeType and HarfBuzz directly and never calls fontconfig. A missing
#     font folder is skipped. Accepted.
#   - libxml2 XML catalog folder: a missing catalog means no catalog lookups. Accepted.
set -euo pipefail
dir=${1:?usage: check-no-nix-store.sh <package-dir>}
[ -d "$dir" ] || { echo "check-no-nix-store: $dir is not a directory" >&2; exit 1; }
allow=${NIX_STORE_ALLOW:-^$}
DATA_DECISIONS=$'^(player|libcrypto\\.3\\.dylib) /nix/store/[a-z0-9]{32}-openssl[^/]*/(etc/ssl(/certs|/private|/engines-3|/ct_log_list\\.cnf)?|lib/(engines-3|ossl-modules))$\tOpenSSL default folders: the CA file is set at runtime (SSL_CERT_FILE), the rest is optional
^lib(av[a-z]+|sw[a-z]+)\\.[0-9]+\\.dylib /nix/store/[a-z0-9]{32}-ffmpeg-[^/]*(/.*)?$\tFFmpeg build configuration text, never opened
^libgnutls\\.[0-9]+\\.dylib /nix/store/[a-z0-9]{32}-gnutls-[^/]*/etc/gnutls/config$\tGnuTLS optional config file, skipped when missing
^lib(gnutls|intl|zvbi)\\.[0-9]+\\.dylib /nix/store/[a-z0-9]{32}-[^/]+/share/locale$\tMessage catalogs, English when missing
^libp11-kit\\.[0-9]+\\.dylib /nix/store/[a-z0-9]{32}-p11-kit-[^/]*/(lib/pkcs11|share/p11-kit/modules)$\tPKCS#11 module folders, no module when missing
^libfontconfig\\.[0-9]+\\.dylib /nix/store/[a-z0-9]{32}-(dejavu-fonts-minimal-[^/]*|fontconfig-[^/]*/share/fontconfig/conf\\.avail)$\tfontconfig built-in config, not used by the player text path
^libxml2\\.[0-9]+\\.dylib /nix/store/[a-z0-9]{32}-libxml2-[^/]*/etc/xml/catalog$\tXML catalog file, no catalog when missing'
bad=0 info=0 unreviewed=0
decide() { # prints the decision for "<name> <path>", or nothing
    local line
    while IFS= read -r line; do
        [[ "$1" =~ ${line%%$'\t'*} ]] && { printf '%s' "${line#*$'\t'}"; return; }
    done <<<"$DATA_DECISIONS"
}
while IFS= read -r f; do
    hits=$(strings -a "$f" | grep -Eo '/nix/store/[a-z0-9]{32}-[^ "<>]*' | sort -u || true)
    [ -n "$hits" ] || continue
    name=$(basename "$f")
    while IFS= read -r p; do
        if [[ "$p" =~ \.(dylib|so|a|dll)$ ]]; then
            if [[ "$name $p" =~ $allow ]]; then
                echo "check-no-nix-store: accepted: $name names $p" >&2
            else
                echo "check-no-nix-store: $name names the library $p" >&2
                bad=1
            fi
        else
            info=$((info + 1))
            why=$(decide "$name $p")
            if [ -n "$why" ]; then
                echo "check-no-nix-store: data path: $name names $p ($why)"
            else
                echo "check-no-nix-store: data path UNREVIEWED: $name names $p"
                unreviewed=$((unreviewed + 1))
            fi
        fi
    done <<<"$hits"
done < <(find "$dir" -type f -not -path '*/licenses/*')
[ "$bad" = 0 ] || exit 1
echo "check-no-nix-store: no /nix/store library path in $dir ($info data or config folder strings, not loadable; $unreviewed unreviewed)"
