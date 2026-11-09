//! dlopen(3) for the decoder libraries, which the helper resolves at run time so no codec binding is linked.
use std::ffi::{c_char, c_int, c_void, CStr, CString};

const RTLD_NOW: c_int = 2;

extern "C" {
    fn dlopen(file: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *mut c_char;
}

/// An open library, never closed: the helper is one process per subject and its exit unloads it.
pub struct Library(*mut c_void);

fn last_error(what: &str) -> String {
    // SAFETY: dlerror returns null or a NUL-terminated string owned by libdl.
    let message = unsafe { dlerror() };
    if message.is_null() {
        what.to_string()
    } else {
        // SAFETY: checked non-null above.
        let detail = unsafe { CStr::from_ptr(message) }.to_string_lossy();
        format!("{what}: {detail}")
    }
}

impl Library {
    pub fn open(name: &str) -> Result<Self, String> {
        let name = CString::new(name).map_err(|error| error.to_string())?;
        // SAFETY: a NUL-terminated name; the handle is checked before use.
        let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW) };
        if handle.is_null() {
            return Err(last_error("dlopen"));
        }
        Ok(Self(handle))
    }

    /// A function pointer of type `T` named `symbol`, which must end in a NUL.
    ///
    /// # Safety
    /// `T` must be the exact `extern "C"` signature the library exports under that name.
    pub unsafe fn get<T: Copy>(&self, symbol: &[u8]) -> Result<T, String> {
        assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<*mut c_void>());
        let name = CStr::from_bytes_with_nul(symbol).map_err(|error| error.to_string())?;
        // SAFETY: a live handle and a NUL-terminated name.
        let found = unsafe { dlsym(self.0, name.as_ptr()) };
        if found.is_null() {
            return Err(last_error(&name.to_string_lossy()));
        }
        // SAFETY: the caller names the exported signature; sizes match by the assert.
        Ok(unsafe { std::mem::transmute_copy(&found) })
    }
}

/// Resolves a list of symbols into one struct of function pointers.
macro_rules! symbols {
    ($library:expr, $name:ident { $($field:ident: $kind:ty = $symbol:literal,)* }) => {
        $name {
            // SAFETY: each type is the library's documented C signature for that symbol.
            $($field: unsafe { $library.get::<$kind>(concat!($symbol, "\0").as_bytes())? },)*
        }
    };
}
pub(crate) use symbols;
