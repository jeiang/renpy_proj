/* THROWAWAY probe (ticket #21): a non-Cython module ("renpy.pygame.surface" stand-in)
   exports PySurface_AsSurface through __pyx_capi__, and an UNMODIFIED Cython consumer
   module that does `from shimpkg.surface cimport PySurface_AsSurface` works against it. */
#define PY_SSIZE_T_CLEAN
#include <Python.h>
#include "shim.h"
typedef struct { PyObject_HEAD SDL_Surface s; } Surf;   /* "Rust Surface": header + SDL-shaped prefix */
static PyObject *surf_new(PyTypeObject *t, PyObject *a, PyObject *k) {
    Surf *o = (Surf *)t->tp_alloc(t, 0);
    o->s.w = 3; o->s.h = 2; o->s.pitch = 16; o->s.pixels = calloc(1, 32);
    return (PyObject *)o;
}
static PyTypeObject SurfT = { PyVarObject_HEAD_INIT(NULL, 0) .tp_name = "shimpkg.surface.Surface",
  .tp_basicsize = sizeof(Surf), .tp_flags = Py_TPFLAGS_DEFAULT | Py_TPFLAGS_BASETYPE, .tp_new = surf_new };
static SDL_Surface *PySurface_AsSurface(PyObject *o) {
    if (!PyObject_TypeCheck(o, &SurfT)) { PyErr_SetString(PyExc_TypeError, "not a Surface"); return NULL; }
    return &((Surf *)o)->s;
}
static PyObject *Surf_bytes(PyObject *self, PyObject *o) {
    Surf *s = (Surf *)o; return PyBytes_FromStringAndSize((char *)s->s.pixels, 8);
}
static PyMethodDef mm[] = {{"first8", Surf_bytes, METH_O, ""}, {0}};
static struct PyModuleDef md = { PyModuleDef_HEAD_INIT, "shimpkg.surface", 0, -1, mm };
static PyObject *PyInit_surface(void) {
    PyObject *m = PyModule_Create(&md);
    PyType_Ready(&SurfT); Py_INCREF(&SurfT); PyModule_AddObject(m, "Surface", (PyObject *)&SurfT);
    PyObject *capi = PyDict_New();
    PyObject *cap = PyCapsule_New((void *)PySurface_AsSurface, "SDL_Surface *(PyObject *)", NULL);
    PyDict_SetItemString(capi, "PySurface_AsSurface", cap);
    PyModule_AddObject(m, "__pyx_capi__", capi);
    return m;
}
int main(void) {
    PyImport_AppendInittab("shimpkg.surface", PyInit_surface);
    Py_Initialize();
    PyRun_SimpleString(
      "import sys; sys.path.insert(0,'.')\n"
      "import shimpkg.surface as S, shimpkg.consumer as C\n"
      "class Sub(S.Surface): pass\n"          /* like pgrender.Surface */
      "s = Sub()\n"
      "print('info', C.info(s)); C.fill(s, 7); print('first8', S.first8(s))\n"
      "try: C.info(object())\n"
      "except TypeError as e: print('type check ok:', e)\n");
    return Py_FinalizeEx();
}
