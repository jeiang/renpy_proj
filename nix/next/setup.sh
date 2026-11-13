set -eu
INC="-I$FFI_DEV/include -I$BZ_DEV/include -I$XZ_DEV/include -I$EXP_DEV/include -I$ZL_DEV/include -I$SSL_DEV/include"
CORE=-DPy_BUILD_CORE_BUILTIN
cat > Modules/Setup.local <<SETUP
*static*
_asyncio $CORE _asynciomodule.c
_bisect $CORE _bisectmodule.c
_blake2 $CORE _blake2/blake2module.c _blake2/blake2b_impl.c _blake2/blake2s_impl.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_bz2 $CORE $INC _bz2module.c $BZ/lib/libbz2.a
_codecs_cn $CORE cjkcodecs/_codecs_cn.c
_codecs_hk $CORE cjkcodecs/_codecs_hk.c
_codecs_iso2022 $CORE cjkcodecs/_codecs_iso2022.c
_codecs_jp $CORE cjkcodecs/_codecs_jp.c
_codecs_kr $CORE cjkcodecs/_codecs_kr.c
_codecs_tw $CORE cjkcodecs/_codecs_tw.c
_contextvars $CORE _contextvarsmodule.c
_csv $CORE _csv.c
_ctypes $CORE $INC -DUSING_MALLOC_CLOSURE_DOT_C -DHAVE_FFI_PREP_CIF_VAR -DHAVE_FFI_PREP_CLOSURE_LOC -DHAVE_FFI_CLOSURE_ALLOC _ctypes/_ctypes.c _ctypes/callbacks.c _ctypes/callproc.c _ctypes/malloc_closure.c _ctypes/stgdict.c _ctypes/cfield.c $FFI/lib/libffi.a
_datetime $CORE _datetimemodule.c
_elementtree $CORE $INC -DUSE_PYEXPAT_CAPI _elementtree.c $EXP/lib/libexpat.a
_hashlib $CORE $INC _hashopenssl.c $SSL/lib/libssl.a $SSL/lib/libcrypto.a
_heapq $CORE _heapqmodule.c
_json $CORE _json.c
_lsprof $CORE _lsprof.c rotatingtree.c
_lzma $CORE $INC _lzmamodule.c $XZ/lib/liblzma.a
_md5 $CORE -I\$(srcdir)/Modules/_hacl/include md5module.c _hacl/Hacl_Hash_MD5.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_multibytecodec $CORE cjkcodecs/multibytecodec.c
_opcode $CORE _opcode.c
_pickle $CORE _pickle.c
_posixsubprocess $CORE _posixsubprocess.c
_queue $CORE _queuemodule.c
_random $CORE _randommodule.c
_sha1 $CORE -I\$(srcdir)/Modules/_hacl/include sha1module.c _hacl/Hacl_Hash_SHA1.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_sha2 $CORE -I\$(srcdir)/Modules/_hacl/include sha2module.c _hacl/Hacl_Hash_SHA2.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_sha3 $CORE -I\$(srcdir)/Modules/_hacl/include sha3module.c _hacl/Hacl_Hash_SHA3.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_socket $CORE socketmodule.c
_ssl $CORE $INC _ssl.c $SSL/lib/libssl.a $SSL/lib/libcrypto.a
_statistics $CORE _statisticsmodule.c
_struct $CORE _struct.c
_typing $CORE _typingmodule.c
_zoneinfo $CORE _zoneinfo.c
array $CORE arraymodule.c
binascii $CORE binascii.c
cmath $CORE cmathmodule.c
fcntl $CORE fcntlmodule.c
grp $CORE grpmodule.c
math $CORE mathmodule.c
mmap $CORE mmapmodule.c
pwd $CORE pwdmodule.c
pyexpat $CORE $INC -DUSE_PYEXPAT_CAPI pyexpat.c $EXP/lib/libexpat.a
resource $CORE resource.c
select $CORE selectmodule.c
termios $CORE termios.c
unicodedata $CORE unicodedata.c
zlib $CORE $INC zlibmodule.c $ZL/lib/libz.a
SETUP
