import sys
sys.stderr = sys.stdout
import ssl, _hashlib, hashlib, ctypes
print("OPENSSL", ssl.OPENSSL_VERSION)
print("sha256", hashlib.sha256(b"x").hexdigest()[:16], "md5 impl", hashlib.md5.__module__ if hasattr(hashlib.md5, "__module__") else "?")
print("ctx", ssl.create_default_context().protocol, "default_verify_paths", ssl.get_default_verify_paths().cafile)
print("hashlib.new('sha512')", hashlib.new("sha512", b"a").hexdigest()[:16], _hashlib.openssl_sha256(b"x").hexdigest()[:16])
