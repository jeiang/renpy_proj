/* THROWAWAY probe (ticket #21): can a host register dotted-name builtin modules
   (PyImport_AppendInittab("fakepkg.sub.native", ...)) that shadow a same-named
   .py file on sys.path, and can Python subclass a type defined there (with a
   buffer export) the way renpy.display.pgrender.Surface subclasses pygame.Surface? */
#define PY_SSIZE_T_CLEAN
#include <Python.h>
typedef struct { PyObject_HEAD unsigned char px[16]; } Surf;
static int surf_getbuf(PyObject *o, Py_buffer *v, int fl) {
    Surf *s = (Surf *)o; return PyBuffer_FillInfo(v, o, s->px, 16, 0, fl);
}
static PyBufferProcs bp = { surf_getbuf, NULL };
static PyObject *surf_size(PyObject *s, PyObject *a) { return Py_BuildValue("(ii)", 2, 2); }
static PyMethodDef sm[] = {{"get_size", surf_size, METH_NOARGS, ""}, {0}};
static PyTypeObject SurfT = { PyVarObject_HEAD_INIT(NULL, 0) .tp_name = "fakepkg.sub.native.Surface",
  .tp_basicsize = sizeof(Surf), .tp_flags = Py_TPFLAGS_DEFAULT | Py_TPFLAGS_BASETYPE,
  .tp_new = PyType_GenericNew, .tp_as_buffer = &bp, .tp_methods = sm };
static struct PyModuleDef md = { PyModuleDef_HEAD_INIT, "fakepkg.sub.native", 0, -1, 0 };
static PyObject *PyInit_native(void) {
    PyObject *m = PyModule_Create(&md);
    if (PyType_Ready(&SurfT) < 0) return NULL;
    Py_INCREF(&SurfT); PyModule_AddObject(m, "Surface", (PyObject *)&SurfT);
    return m;
}
int main(void) {
    PyImport_AppendInittab("fakepkg.sub.native", PyInit_native);
    Py_Initialize();
    PyRun_SimpleString(
      "import sys; sys.path.insert(0,'pkg')\n"
      "import fakepkg.sub.native as n\n"
      "from fakepkg.sub import native\n"
      "print('module', n, n is native, n.__spec__.origin)\n"
      "class S(n.Surface):\n"
      "    def copy(self): return 'py-override'\n"
      "s = S(); print('subclass ok', s.get_size(), s.copy(), len(bytes(memoryview(s))))\n");
    return Py_FinalizeEx();
}
