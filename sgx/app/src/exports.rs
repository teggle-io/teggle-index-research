use std::ffi::c_void;
use enclave_ffi_types::UserSpaceBuffer;

#[no_mangle]
pub extern "C" fn ocall_allocate(buffer: *const u8, length: usize) -> UserSpaceBuffer {
    let slice = unsafe { std::slice::from_raw_parts(buffer, length) };
    let vector_copy = slice.to_vec();
    let boxed_vector = Box::new(vector_copy);
    let heap_pointer = Box::into_raw(boxed_vector);
    UserSpaceBuffer {
        ptr: heap_pointer as *mut c_void,
    }
}