//! Encryption helpers shared by commands that store secrets (items and project overviews) and
//! by backups, which re-seal a whole vault with the same key.

use zeroize::Zeroizing;

use crate::{
    crypto::{self, VaultKey},
    db::SecureBody,
    error::{AppError, AppResult},
    secrets,
    state::AppState,
};

/// Encrypts with the session key; fails with `Locked` when the unlock window is closed.
pub fn seal(state: &AppState, plaintext: &str) -> AppResult<SecureBody> {
    seal_with(&state.session().active_key()?, plaintext)
}

/// Splits the `` `!...!` `` spans out of `body` and encrypts them with the session key.
/// Returns `None` when the body has no secrets.
pub fn seal_inline(state: &AppState, body: &str) -> AppResult<Option<(String, SecureBody)>> {
    seal_inline_with(&state.session().active_key()?, body)
}

/// Decrypts a sealed body with the session key. With a template, the decrypted spans are put
/// back into it.
pub fn open(state: &AppState, template: Option<&str>, sealed: &SecureBody) -> AppResult<String> {
    Ok(open_with(&state.session().active_key()?, template, sealed)?.to_string())
}

pub fn seal_with(key: &VaultKey, plaintext: &str) -> AppResult<SecureBody> {
    let (nonce, ciphertext) = crypto::encrypt(key, plaintext)?;
    Ok(SecureBody { nonce, ciphertext })
}

/// Like `seal_inline`, with an explicit key.
pub fn seal_inline_with(key: &VaultKey, body: &str) -> AppResult<Option<(String, SecureBody)>> {
    let (template, spans) = secrets::extract(body);
    let spans = Zeroizing::new(spans);
    if spans.is_empty() {
        return Ok(None);
    }
    let json = Zeroizing::new(
        serde_json::to_string(&*spans).expect("a list of strings always serializes"),
    );
    Ok(Some((template, seal_with(key, &json)?)))
}

/// Like `open`, with an explicit key.
pub fn open_with(
    key: &VaultKey,
    template: Option<&str>,
    sealed: &SecureBody,
) -> AppResult<Zeroizing<String>> {
    let Some(template) = template else {
        return crypto::decrypt(key, &sealed.nonce, &sealed.ciphertext);
    };
    Ok(Zeroizing::new(secrets::restore(
        template,
        &open_spans(key, sealed)?,
    )))
}

/// The secret values sealed for a template, in marker order.
pub fn open_spans(key: &VaultKey, sealed: &SecureBody) -> AppResult<Zeroizing<Vec<String>>> {
    let plaintext = crypto::decrypt(key, &sealed.nonce, &sealed.ciphertext)?;
    Ok(Zeroizing::new(serde_json::from_str(&plaintext).map_err(
        |_| AppError::Crypto("Corrupted secrets".to_string()),
    )?))
}
