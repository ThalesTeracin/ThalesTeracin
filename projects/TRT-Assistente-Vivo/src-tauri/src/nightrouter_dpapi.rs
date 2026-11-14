use std::path::PathBuf;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
};

pub fn read_dpapi_key(filename: &str) -> Option<String> {
    let home = std::env::var_os("LOCALAPPDATA")?;
    let path = PathBuf::from(home)
        .join("TRT-Assistente-Vivo")
        .join("secrets")
        .join(filename);
    let mut encrypted = std::fs::read(path).ok()?;
    if encrypted.len() < 8 || encrypted.len() > 16384 {
        return None;
    }
    let data_len = u32::try_from(encrypted.len()).ok()?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: data_len,
        pbData: encrypted.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let status = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if status == 0 || output.pbData.is_null() {
        return None;
    }
    let plain = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe { LocalFree(output.pbData.cast()); }
    String::from_utf8(plain).ok().filter(|s| s.len() >= 12 && s.len() <= 4096)
}


pub fn read_nightrouter_key() -> Option<String> { read_dpapi_key("nightrouter.dpapi") }
