//! Encrypted scratches and the app PIN.
//!
//! An encrypted file is `MAGIC`, the vault salt, a fresh nonce, then the
//! text sealed with XChaCha20-Poly1305. The key comes from the user's
//! password through Argon2id and lives only in memory, wiped on lock.
//! The PIN is stored as an Argon2id hash and only guards the window.

use std::fmt::Write as _;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

/// First bytes of every encrypted file. Text files never start with them
/// by accident, and other programs show a clear hint instead of noise.
pub const MAGIC: &[u8] = b"RUBRIQUE-ENC1\n";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
/// Sealed with the key at setup, so a wrong password is caught at once.
const CHECK_TEXT: &[u8] = b"rubrique vault";

/// The file starts with [`MAGIC`].
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// The file on disk is encrypted (reads only its first bytes).
pub fn file_is_encrypted(path: &Path) -> bool {
    let mut head = [0; MAGIC.len()];
    File::open(path).and_then(|mut file| file.read_exact(&mut head)).is_ok() && head == MAGIC
}

/// Why encrypted bytes could not be opened.
#[derive(Debug, PartialEq, Eq)]
pub enum DecryptError {
    /// Sealed with another password (or changed since).
    WrongKey,
    /// Not an encrypted file, or cut short.
    Corrupt,
}

/// What settings keep about the vault: the salt and the sealed check text.
/// Neither tells anything about the password on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultInfo {
    salt: [u8; SALT_LEN],
    check: Vec<u8>,
}

impl VaultInfo {
    /// `salt-hex:check-hex`, the form in the settings file.
    pub fn to_text(&self) -> String {
        format!("{}:{}", to_hex(&self.salt), to_hex(&self.check))
    }

    /// Read [`VaultInfo::to_text`]; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let (salt, check) = text.split_once(':')?;
        let salt = from_hex(salt)?.try_into().ok()?;
        let check = from_hex(check)?;
        (!check.is_empty()).then_some(VaultInfo { salt, check })
    }
}

/// The unlocked key for encrypted scratches.
pub struct Vault {
    key: Zeroizing<[u8; KEY_LEN]>,
    salt: [u8; SALT_LEN],
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Vault(unlocked)")
    }
}

impl Vault {
    /// A new vault for `password` with a random salt, and what to store.
    pub fn create(password: &str) -> (Vault, VaultInfo) {
        let mut salt = [0; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let vault = Vault { key: derive_key(password, &salt), salt };
        let check = vault.encrypt(CHECK_TEXT);
        (vault, VaultInfo { salt, check })
    }

    /// Open the vault with `password`; `None` when the password is wrong.
    pub fn unlock(password: &str, info: &VaultInfo) -> Option<Vault> {
        let vault = Vault { key: derive_key(password, &info.salt), salt: info.salt };
        (vault.decrypt(&info.check).ok()?.as_slice() == CHECK_TEXT).then_some(vault)
    }

    /// Open the vault from an encrypted file when the settings lost what
    /// [`Vault::create`] stored: the salt is in the file too.
    pub fn recover(password: &str, file: &[u8]) -> Option<(Vault, VaultInfo)> {
        let salt: [u8; SALT_LEN] = file.strip_prefix(MAGIC)?.get(..SALT_LEN)?.try_into().ok()?;
        let vault = Vault { key: derive_key(password, &salt), salt };
        vault.decrypt(file).ok()?;
        let check = vault.encrypt(CHECK_TEXT);
        Some((vault, VaultInfo { salt, check }))
    }

    /// Seal `plain` with a fresh nonce.
    pub fn encrypt(&self, plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let sealed = self
            .cipher()
            .encrypt(XNonce::from_slice(&nonce), plain)
            // Only fails for inputs far larger than any file we open.
            .unwrap_or_default();
        let mut bytes = Vec::with_capacity(MAGIC.len() + SALT_LEN + NONCE_LEN + sealed.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&self.salt);
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&sealed);
        bytes
    }

    /// Open bytes made by [`Vault::encrypt`].
    ///
    /// # Errors
    ///
    /// [`DecryptError::WrongKey`] for another key or changed bytes,
    /// [`DecryptError::Corrupt`] for bytes that are not an encrypted file.
    pub fn decrypt(&self, bytes: &[u8]) -> Result<Vec<u8>, DecryptError> {
        let rest = bytes.strip_prefix(MAGIC).ok_or(DecryptError::Corrupt)?;
        if rest.len() < SALT_LEN + NONCE_LEN {
            return Err(DecryptError::Corrupt);
        }
        let (salt, rest) = rest.split_at(SALT_LEN);
        let (nonce, sealed) = rest.split_at(NONCE_LEN);
        if salt != self.salt {
            return Err(DecryptError::WrongKey);
        }
        self.cipher().decrypt(XNonce::from_slice(nonce), sealed).map_err(|_| DecryptError::WrongKey)
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(self.key.as_ref().into())
    }
}

/// Argon2id settings: the library defaults (19 MiB, 2 passes) in the app,
/// tiny ones in tests so they stay fast.
fn argon2() -> Argon2<'static> {
    let params =
        if cfg!(test) { Params::new(8, 1, 1, Some(KEY_LEN)) } else { Params::new(19 * 1024, 2, 1, Some(KEY_LEN)) };
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params.unwrap_or_default())
}

fn derive_key(password: &str, salt: &[u8]) -> Zeroizing<[u8; KEY_LEN]> {
    let mut key = Zeroizing::new([0; KEY_LEN]);
    // Fails only for salts or outputs of the wrong length, which are fixed here.
    let _ = argon2().hash_password_into(password.as_bytes(), salt, key.as_mut());
    key
}

/// The PIN as an Argon2id hash string for the settings file.
pub fn pin_hash(pin: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    argon2().hash_password(pin.as_bytes(), &salt).map(|hash| hash.to_string()).unwrap_or_default()
}

/// `pin` matches the stored [`pin_hash`].
pub fn verify_pin(pin: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| argon2().verify_password(pin.as_bytes(), &parsed).is_ok())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len()).step_by(2).map(|start| u8::from_str_radix(text.get(start..start + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    #[test]
    fn round_trip_and_fresh_nonces() {
        let (vault, _) = Vault::create("correct horse");
        let first = vault.encrypt("secret note".as_bytes());
        let second = vault.encrypt("secret note".as_bytes());
        assert!(is_encrypted(&first));
        assert_ne!(first, second);
        assert!(!first.windows(6).any(|window| window == b"secret"));
        assert_eq!(vault.decrypt(&first).unwrap(), b"secret note");
        assert_eq!(vault.decrypt(&vault.encrypt(b"")).unwrap(), b"");
    }

    #[test]
    fn unlock_checks_the_password() {
        let (_, info) = Vault::create("correct horse");
        let info = VaultInfo::parse(&info.to_text()).unwrap();
        assert!(Vault::unlock("wrong", &info).is_none());
        assert!(Vault::unlock("correct horse", &info).is_some());
    }

    #[test]
    fn other_keys_and_changed_bytes_fail() {
        let (vault, _) = Vault::create("a");
        let (other, _) = Vault::create("a");
        let mut bytes = vault.encrypt(b"text");
        assert_eq!(other.decrypt(&bytes), Err(DecryptError::WrongKey));
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(vault.decrypt(&bytes), Err(DecryptError::WrongKey));
        assert_eq!(vault.decrypt(b"plain text"), Err(DecryptError::Corrupt));
        assert_eq!(vault.decrypt(MAGIC), Err(DecryptError::Corrupt));
    }

    #[test]
    fn recovers_from_a_file() {
        let (vault, info) = Vault::create("pw");
        let file = vault.encrypt(b"x");
        assert!(Vault::recover("wrong", &file).is_none());
        let (_, found) = Vault::recover("pw", &file).unwrap();
        assert!(Vault::unlock("pw", &found).is_some());
        assert_eq!(found.salt, info.salt);
        assert!(Vault::recover("pw", b"plain").is_none());
    }

    #[test]
    fn pin_hash_verifies() {
        let hash = pin_hash("1234");
        assert!(verify_pin("1234", &hash));
        assert!(!verify_pin("1235", &hash));
        assert!(!verify_pin("1234", "garbage"));
    }

    #[test]
    fn vault_info_rejects_garbage() {
        assert_eq!(VaultInfo::parse(""), None);
        assert_eq!(VaultInfo::parse("zz:00"), None);
        assert_eq!(VaultInfo::parse("00:00"), None);
    }

    #[test]
    fn detects_encrypted_files() {
        let temp = TempFolder::new();
        let (vault, _) = Vault::create("a");
        let sealed = temp.path().join("s.md");
        std::fs::write(&sealed, vault.encrypt(b"x")).unwrap();
        let plain = temp.write("p.md", "# hi");
        assert!(file_is_encrypted(&sealed));
        assert!(!file_is_encrypted(&plain));
        assert!(!file_is_encrypted(&temp.path().join("missing.md")));
    }
}
