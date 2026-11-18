use std::ffi::CStr;

fn main() {
    let digest = ring::digest::digest(&ring::digest::SHA256, b"mm");
    let sqlite = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_libversion()) };
    println!(
        "cross-check {} {} sqlite {} sha256 {:02x}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH,
        sqlite.to_string_lossy(),
        digest.as_ref()[0]
    );
}
