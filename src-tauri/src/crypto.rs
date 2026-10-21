use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, Key, KeyInit},
};
use security_framework::passwords::{get_generic_password, set_generic_password};
use zeroize::{Zeroize, Zeroizing};

use crate::error::{AppError, AppResult};

const KEYCHAIN_SERVICE: &str = "com.apo.coffer";
const KEYCHAIN_ACCOUNT: &str = "vault-key";
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

pub type VaultKey = Zeroizing<Vec<u8>>;

/// Loads the vault key from the Keychain, creating one on first run.
/// Any error other than "not found" is returned as-is so an existing key is never overwritten.
pub fn load_or_create_key() -> AppResult<VaultKey> {
    match get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(bytes) => Ok(Zeroizing::new(bytes)),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => {
            let key = Key::<Aes256Gcm>::generate();
            let bytes = Zeroizing::new(key.to_vec());
            set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, &bytes)
                .map_err(|e| AppError::Keychain(format!("Could not store vault key: {e}")))?;
            Ok(bytes)
        }
        Err(e) => Err(AppError::Keychain(format!("Could not read vault key: {e}"))),
    }
}

fn crypto_error(message: &str) -> AppError {
    AppError::Crypto(message.to_string())
}

fn cipher(key: &VaultKey) -> AppResult<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key).map_err(|_| crypto_error("Invalid vault key"))
}

/// Returns (nonce, ciphertext).
pub fn encrypt(key: &VaultKey, plaintext: &str) -> AppResult<(Vec<u8>, Vec<u8>)> {
    let nonce = Nonce::generate();
    let ciphertext = cipher(key)?
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| crypto_error("Encryption failed"))?;
    Ok((nonce.to_vec(), ciphertext))
}

pub fn decrypt(key: &VaultKey, nonce: &[u8], ciphertext: &[u8]) -> AppResult<Zeroizing<String>> {
    let nonce = Nonce::try_from(nonce).map_err(|_| crypto_error("Corrupted nonce"))?;
    let mut bytes = Zeroizing::new(
        cipher(key)?
            .decrypt(&nonce, ciphertext)
            .map_err(|_| crypto_error("Decryption failed"))?,
    );
    // Move the buffer into the String (no copy) so the plaintext only ever lives zeroized.
    String::from_utf8(std::mem::take(&mut *bytes))
        .map(Zeroizing::new)
        .map_err(|e| {
            let mut leftover = e.into_bytes();
            leftover.zeroize();
            crypto_error("Corrupted note")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> VaultKey {
        Zeroizing::new(Key::<Aes256Gcm>::generate().to_vec())
    }

    #[test]
    fn round_trip() {
        let key = test_key();
        let (nonce, ciphertext) = encrypt(&key, "sk_live_123 🔑").unwrap();
        assert_eq!(
            decrypt(&key, &nonce, &ciphertext).unwrap().as_str(),
            "sk_live_123 🔑"
        );
    }

    #[test]
    fn wrong_key_fails() {
        let (nonce, ciphertext) = encrypt(&test_key(), "secret").unwrap();
        assert!(decrypt(&test_key(), &nonce, &ciphertext).is_err());
    }
}
