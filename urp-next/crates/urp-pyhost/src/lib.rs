use std::{ffi::CString, marker::PhantomData, mem::MaybeUninit, rc::Rc, sync::atomic::{AtomicBool, Ordering}};
use pyo3_ffi as ffi;

include!(concat!(env!("OUT_DIR"), "/native.rs"));

static STARTED: AtomicBool = AtomicBool::new(false);

/// One interpreter on its creating thread, for the lifetime of the process.
pub struct Interpreter {
    _thread: PhantomData<Rc<()>>,
}

impl Interpreter {
    pub fn initialize() -> Result<Self, String> {
        if STARTED.swap(true, Ordering::AcqRel) {
            return Err("the interpreter can only be initialized once per process".into());
        }
        unsafe {
            if ffi::Py_IsInitialized() != 0 {
                return Err("another caller has already initialized CPython".into());
            }
            if register() != 0 {
                return Err("registering engine builtins failed".into());
            }
            let mut config = MaybeUninit::<ffi::PyConfig>::uninit();
            ffi::PyConfig_InitIsolatedConfig(config.as_mut_ptr());
            let mut config = config.assume_init();
            config.install_signal_handlers = 0;
            let home = CString::new(HOME).unwrap();
            let mut status = ffi::PyConfig_SetBytesString(&mut config, &mut config.home, home.as_ptr());
            if ffi::PyStatus_Exception(status) == 0 {
                status = ffi::Py_InitializeFromConfig(&config);
            }
            let error = if ffi::PyStatus_Exception(status) != 0 {
                Some(if status.err_msg.is_null() {
                    "CPython initialization failed".to_owned()
                } else {
                    std::ffi::CStr::from_ptr(status.err_msg).to_string_lossy().into_owned()
                })
            } else { None };
            ffi::PyConfig_Clear(&mut config);
            if let Some(error) = error { return Err(error); }
        }
        let interpreter = Self { _thread: PhantomData };
        interpreter.run(PATHS)?;
        interpreter.run(include_str!("../../../../nix/next/bootstrap.py"))?;
        Ok(interpreter)
    }

    /// Executes code on the interpreter's owning thread. Python errors print their traceback.
    pub fn run(&self, source: &str) -> Result<(), String> {
        let source = CString::new(source).map_err(|_| "Python source contains a NUL byte")?;
        if unsafe { ffi::PyRun_SimpleString(source.as_ptr()) } != 0 {
            Err("Python execution failed (see traceback)".into())
        } else { Ok(()) }
    }

    pub fn finalize(self) -> Result<(), String> {
        let status = unsafe { ffi::Py_FinalizeEx() };
        std::mem::forget(self);
        if status == 0 { Ok(()) } else { Err("CPython finalization failed".into()) }
    }
}

impl Drop for Interpreter {
    fn drop(&mut self) {
        unsafe { ffi::Py_FinalizeEx(); }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn boots_renpy() {
        let interpreter = super::Interpreter::initialize().unwrap();
        interpreter.run("import renpy; assert renpy.version_tuple[:3] == (8, 5, 3)").unwrap();
        interpreter.finalize().unwrap();
    }
}
