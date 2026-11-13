#include <Python.h>
#include "inittab.h"
#include "bootstrap.h"

int main(int argc, char **argv) {
    if (PyImport_ExtendInittab(engine_inittab) < 0) return 1;
    PyConfig config;
    PyConfig_InitPythonConfig(&config);
    config.use_environment = 0;
    PyStatus status = PyConfig_SetBytesString(&config, &config.home, CPYTHON);
    if (!PyStatus_Exception(status)) status = PyConfig_SetBytesArgv(&config, argc, argv);
    if (!PyStatus_Exception(status)) status = Py_InitializeFromConfig(&config);
    PyConfig_Clear(&config);
    if (PyStatus_Exception(status)) Py_ExitStatusException(status);
    if (PyRun_SimpleString("import sys; sys.path.extend([" ENGINE_TREE ", " ENGINE_LAYER "])") != 0 ||
        PyRun_SimpleString(bootstrap) != 0) {
        Py_FinalizeEx();
        return 1;
    }
    return Py_RunMain();
}
