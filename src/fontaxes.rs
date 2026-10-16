// FreeType's public MM ABI, loaded only in a bounded selected-font helper process.
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use crate::jsondoc::Json;

#[repr(C)]
struct Axis { name: *const c_char, minimum: c_long, default: c_long, maximum: c_long, tag: c_ulong, strid: c_uint }
#[repr(C)]
struct Variation { count: c_uint, designs: c_uint, styles: c_uint, axes: *const Axis, named: *const c_void }
extern "C" {
    fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}
struct Library(*mut c_void);
impl Drop for Library { fn drop(&mut self) { unsafe { dlclose(self.0); } } }
type Init = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
type NewFace = unsafe extern "C" fn(*mut c_void, *const c_char, c_long, *mut *mut c_void) -> c_int;
type GetVar = unsafe extern "C" fn(*mut c_void, *mut *mut Variation) -> c_int;
type DoneVar = unsafe extern "C" fn(*mut c_void, *mut Variation) -> c_int;
type Done = unsafe extern "C" fn(*mut c_void) -> c_int;
struct Owned { pointer: *mut c_void, done: Done }
impl Drop for Owned { fn drop(&mut self) { unsafe { (self.done)(self.pointer); } } }
struct OwnedVar { library: *mut c_void, pointer: *mut Variation, done: DoneVar }
impl Drop for OwnedVar { fn drop(&mut self) { unsafe { (self.done)(self.library, self.pointer); } } }
fn symbol(library: &Library, name: &std::ffi::CStr) -> Result<*mut c_void, String> {
    let found = unsafe { dlsym(library.0, name.as_ptr()) };
    if found.is_null() { Err(format!("FreeType is missing {}.", name.to_string_lossy())) } else { Ok(found) }
}
fn number(value: c_long) -> Json { Json::Num((value as f64 / 65536.0).to_string()) }

pub fn inspect(path: &Path, indices: &str) -> Result<String, String> {
    let indices: Vec<u32> = indices.split(',').map(|s| s.parse::<u32>().map_err(|_| "Invalid font face index.".to_string())).collect::<Result<_, _>>()?;
    if indices.is_empty() || indices.len() > 256 || indices.iter().any(|i| *i > 65535) { return Err("Invalid font face indices.".into()); }
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid font path.")?;
    let raw = unsafe { dlopen(c"libfreetype.so.6".as_ptr(), 2) };
    if raw.is_null() { return Err("Variable font inspection needs FreeType.".into()); }
    let library = Library(raw);
    // All signatures match FreeType's public freetype.h / ftmm.h declarations.
    let init: Init = unsafe { std::mem::transmute(symbol(&library, c"FT_Init_FreeType")?) };
    let new_face: NewFace = unsafe { std::mem::transmute(symbol(&library, c"FT_New_Face")?) };
    let get_var: GetVar = unsafe { std::mem::transmute(symbol(&library, c"FT_Get_MM_Var")?) };
    let done_var: DoneVar = unsafe { std::mem::transmute(symbol(&library, c"FT_Done_MM_Var")?) };
    let done_face: Done = unsafe { std::mem::transmute(symbol(&library, c"FT_Done_Face")?) };
    let done_library: Done = unsafe { std::mem::transmute(symbol(&library, c"FT_Done_FreeType")?) };
    let mut pointer = std::ptr::null_mut();
    if unsafe { init(&mut pointer) } != 0 { return Err("FreeType could not initialize.".into()); }
    let owner = Owned { pointer, done: done_library };
    let mut result = Vec::new();
    for index in indices {
        let mut pointer = std::ptr::null_mut();
        if unsafe { new_face(owner.pointer, path.as_ptr(), index.into(), &mut pointer) } != 0 { return Err("FreeType could not open the font face.".into()); }
        let face = Owned { pointer, done: done_face };
        let mut variation = std::ptr::null_mut();
        let code = unsafe { get_var(face.pointer, &mut variation) };
        if variation.is_null() { return Err("FreeType could not read variable axes.".into()); }
        let variation = OwnedVar { library: owner.pointer, pointer: variation, done: done_var };
        if code != 0 { return Err("FreeType could not read variable axes.".into()); }
        let header = unsafe { &*variation.pointer };
        if header.count > 64 || (header.count > 0 && header.axes.is_null()) { return Err("Invalid variable font axes.".into()); }
        let mut axes = Vec::new();
        for i in 0..header.count as usize {
            let axis = unsafe { &*header.axes.add(i) };
            let tag = u32::try_from(axis.tag).map_err(|_| "Invalid variable axis tag.")?.to_be_bytes();
            if !tag.iter().all(|b| (32..=126).contains(b)) { return Err("Invalid variable axis tag.".into()); }
            axes.push(Json::Obj(vec![("tag".into(), Json::Str(String::from_utf8(tag.to_vec()).unwrap())),
                ("minimum".into(), number(axis.minimum)), ("default".into(), number(axis.default)), ("maximum".into(), number(axis.maximum))]));
        }
        result.push((index.to_string(), Json::Arr(axes)));
    }
    Ok(crate::jsondoc::render(&Json::Obj(result)))
}
