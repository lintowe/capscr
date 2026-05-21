// secret-at-rest for capscr's config.toml. Wraps Win32 DPAPI
// (CryptProtectData / CryptUnprotectData) so values are scoped to the
// current user account — copying config.toml to another machine or
// another user account makes the encrypted blob unrecoverable.
//
// On non-Windows targets the module exposes the same surface but stores
// values plaintext. capscr ships Windows-only so this fallback is for
// tests + the (vestigial) Linux cargo target hooks.

use anyhow::{anyhow, Result};

/// encrypt `plaintext` and return a hex-encoded blob safe to drop into
/// config.toml. The blob is bound to the current user account.
pub fn encrypt(plaintext: &str) -> Result<String> {
    #[cfg(windows)]
    {
        encrypt_win(plaintext)
    }
    #[cfg(not(windows))]
    {
        Ok(hex::encode(plaintext.as_bytes()))
    }
}

/// decrypt a blob previously produced by `encrypt`.
pub fn decrypt(blob: &str) -> Result<String> {
    #[cfg(windows)]
    {
        decrypt_win(blob)
    }
    #[cfg(not(windows))]
    {
        let bytes = hex::decode(blob).map_err(|e| anyhow!("bad hex: {e}"))?;
        Ok(String::from_utf8(bytes).map_err(|e| anyhow!("bad utf-8: {e}"))?)
    }
}

#[cfg(windows)]
fn encrypt_win(plaintext: &str) -> Result<String> {
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    use windows::Win32::Foundation::HLOCAL;

    let mut input = plaintext.as_bytes().to_vec();
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_mut_ptr(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    let entropy = b"capscr/config/v1".to_vec();
    let mut entropy_mut = entropy.clone();
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy_mut.len() as u32,
        pbData: entropy_mut.as_mut_ptr(),
    };
    unsafe {
        CryptProtectData(
            &in_blob,
            None,
            Some(&entropy_blob),
            None,
            None,
            0,
            &mut out_blob,
        )
        .map_err(|e| anyhow!("CryptProtectData: {e}"))?;
    }
    let slice = unsafe {
        std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec()
    };
    unsafe {
        let _ = LocalFree(HLOCAL(out_blob.pbData as *mut _));
    }
    Ok(hex::encode(slice))
}

#[cfg(windows)]
fn decrypt_win(blob: &str) -> Result<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    use windows::Win32::Foundation::HLOCAL;

    let mut bytes = hex::decode(blob).map_err(|e| anyhow!("bad hex: {e}"))?;
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    };
    let mut entropy = b"capscr/config/v1".to_vec();
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_mut_ptr(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    let mut desc = PWSTR::null();
    unsafe {
        CryptUnprotectData(
            &in_blob,
            Some(&mut desc),
            Some(&entropy_blob),
            None,
            None,
            0,
            &mut out_blob,
        )
        .map_err(|e| anyhow!("CryptUnprotectData: {e}"))?;
    }
    let plaintext_bytes = unsafe {
        std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec()
    };
    unsafe {
        let _ = LocalFree(HLOCAL(out_blob.pbData as *mut _));
        if !desc.is_null() {
            let _ = LocalFree(HLOCAL(desc.0 as *mut _));
        }
    }
    String::from_utf8(plaintext_bytes).map_err(|e| anyhow!("bad utf-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let plain = "hunter2 — with spaces and unicode ✓";
        let blob = encrypt(plain).expect("encrypt");
        assert_ne!(blob, plain, "blob must not equal plaintext");
        let back = decrypt(&blob).expect("decrypt");
        assert_eq!(back, plain);
    }
}
