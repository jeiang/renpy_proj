import sys
import _imp
from importlib.machinery import ModuleSpec, BuiltinImporter


class BuiltinSubmoduleImporter:
    @staticmethod
    def find_spec(fullname, path=None, target=None):
        if "." in fullname and _imp.is_builtin(fullname):
            return ModuleSpec(fullname, BuiltinImporter, origin="built-in")
        return None


sys.meta_path.insert(0, BuiltinSubmoduleImporter)
