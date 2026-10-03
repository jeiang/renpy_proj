# Probe for player/packaging/macos.sh. It is copied into a throwaway game as game/script.rpy.
# The OpenSSL in the bundle names a default certificate file under /nix/store, which does not exist on a Mac
# without Nix. The player sets SSL_CERT_FILE to the system bundle at start. This checks, from the bundled
# interpreter, that `ssl` finds an existing certificate file outside /nix/store and loads certificates from it.
# When the machine has network, it also does a verified TLS handshake. It writes the result to $SSL_PROBE_OUT
# and exits at init time, before any window opens.

init python:
    import os
    import socket
    import ssl

    def _probe():
        out = os.environ["SSL_PROBE_OUT"]

        try:
            paths = ssl.get_default_verify_paths()
            cafile = paths.cafile or ""

            if not cafile or cafile.startswith("/nix/store") or not os.path.isfile(cafile):
                raise RuntimeError("default cafile %r (capath %r, openssl_cafile %r) is missing or in /nix/store"
                                   % (paths.cafile, paths.capath, paths.openssl_cafile))

            ctx = ssl.create_default_context()
            count = len(ctx.get_ca_certs())

            if count == 0:
                raise RuntimeError("no certificate loaded from %s" % cafile)

            result = "ssl probe ok: cafile %s, %d certificates" % (cafile, count)

            try:
                sock = socket.create_connection(("github.com", 443), timeout=15)
            except OSError as e:
                result += "; handshake skipped (no network: %s)" % e
            else:
                with sock:
                    with ctx.wrap_socket(sock, server_hostname="github.com") as tls:
                        result += "; verified handshake with github.com (%s)" % tls.version()
        except BaseException as e:
            result = "ssl probe failed: %s: %s" % (type(e).__name__, e)

        with open(out, "w") as f:
            f.write(result + "\n")

        os._exit(0)

    _probe()

label start:
    return
