#[no_mangle]
pub unsafe extern "C" fn ecall_init() {
    #[cfg(not(feature = "production"))]
    pretty_env_logger::init();
}