//! DeepSeek API key resolution:
//!   1. deepSeekApiKey saved directly from the settings UI
//!   2. DEEPSEEK_API_KEY environment variable
//!
//! The key is read into memory only when a request is about to be made and is
//! never logged, cached to disk, or sent to the renderer.

use crate::config::Config;

pub fn deepseek_key(config: &Config) -> Option<String> {
    let k = config.deep_seek_api_key.trim().to_string();
    if !k.is_empty() {
        return Some(k);
    }
    if let Ok(k) = std::env::var("DEEPSEEK_API_KEY") {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Some(k);
        }
    }
    None
}

/// Resolve the Mistral credential without exposing it to the renderer.
pub fn mistral_key() -> Option<String> {
    if let Ok(value) = std::env::var("MISTRAL_API_KEY") {
        let value = value.trim().to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    if let Some(value) = mistral_key_from_env_file() {
        return Some(value);
    }
    mistral_key_from_os_keyring()
}

fn mistral_key_from_env_file() -> Option<String> {
    let path = dirs::home_dir()?.join(".vibe").join(".env");
    let text = std::fs::read_to_string(path).ok()?;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != "MISTRAL_API_KEY" {
            continue;
        }
        let value = value.trim().trim_matches(|ch| ch == '"' || ch == '\'');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(windows)]
fn mistral_key_from_os_keyring() -> Option<String> {
    read_windows_vibe_credential()
}

#[cfg(not(windows))]
fn mistral_key_from_os_keyring() -> Option<String> {
    None
}

#[cfg(windows)]
fn read_windows_vibe_credential() -> Option<String> {
    for target in [
        "ai.mistral.vibe",
        "MISTRAL_API_KEY@ai.mistral.vibe",
        "vibe",
        "MISTRAL_API_KEY@vibe",
    ] {
        if let Some(value) = read_windows_generic_credential(target) {
            return Some(value);
        }
    }
    None
}

#[cfg(windows)]
fn read_windows_generic_credential(target: &str) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let encoded: Vec<u16> = std::ffi::OsStr::new(target)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut credential = std::ptr::null_mut();
    unsafe {
        if CredReadW(
            PCWSTR(encoded.as_ptr()),
            CRED_TYPE_GENERIC,
            0,
            &mut credential,
        )
        .is_err()
            || credential.is_null()
        {
            return None;
        }
    }

    struct CredGuard(*mut CREDENTIALW);
    impl Drop for CredGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CredFree(self.0 as *const _) };
            }
        }
    }

    let guard = CredGuard(credential);
    let cred = unsafe { &*guard.0 };
    if cred.CredentialBlobSize == 0 || cred.CredentialBlob.is_null() {
        return None;
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize)
    };

    // python-keyring writes Windows vault passwords as UTF-16. Keep UTF-8 as a
    // compatibility fallback for credentials written by other clients.
    let looks_utf16 =
        bytes.len().is_multiple_of(2) && bytes.chunks_exact(2).any(|pair| pair[1] == 0);
    if looks_utf16 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        if let Ok(value) = String::from_utf16(&units) {
            let value = value.trim_matches('\0').trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    String::from_utf8(bytes.to_vec())
        .ok()
        .map(|value| value.trim_matches('\0').trim().to_string())
        .filter(|value| !value.is_empty())
}
