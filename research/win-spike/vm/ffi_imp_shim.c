/* PBS's _ctypes objects reference libffi through dllimport (__imp_ffi_*). Static libffi defines plain ffi_*: point the __imp_ slots at them. */
#define S(n) extern char n; void *__imp_##n = &n;
S(ffi_call) S(ffi_prep_cif) S(ffi_prep_closure)
S(ffi_type_double) S(ffi_type_float) S(ffi_type_pointer) S(ffi_type_sint16) S(ffi_type_sint32) S(ffi_type_sint64) S(ffi_type_sint8)
S(ffi_type_uint16) S(ffi_type_uint32) S(ffi_type_uint64) S(ffi_type_uint8) S(ffi_type_void)
