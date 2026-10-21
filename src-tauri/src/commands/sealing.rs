//! Encryption helpers shared by commands that store secrets (items and project overviews).

use zeroize::Zeroizing;

use crate::{
    crypto,
    db::SecureBody,
    error::{AppError, AppResult},
    secrets,
    state::AppState,
};

/// Encrypts with the session key; fails with `Locked` when the unlock window is closed.
pub fn seal(state: &AppState, plaintext: &str) -> AppResult<SecureBody> {
    let (nonce, ciphertext) = crypto::encrypt(&state.session().active_key()?, plaintext)?;
    Ok(SecureBody { nonce, ciphertext })
}

/// Splits the `` `!...!` `` spans out of `body` and encrypts them as one list.
/// Returns `None` when the body has no secrets.
pub fn seal_inline(state: &AppState, body: &str) -> AppResult<Option<(String, SecureBody)>> {
    let (template, spans) = secrets::extract(body);
    let spans = Zeroizing::new(spans);
    if spans.is_empty() {
        return Ok(None);
    }
    let json = Zeroizing::new(
        serde_json::to_string(&*spans).expect("a list of strings always serializes"),
    );
    Ok(Some((template, seal(state, &json)?)))
}

/// Decrypts a sealed body. With a template, the decrypted spans are put back into it.
pub fn open(state: &AppState, template: Option<&str>, sealed: &SecureBody) -> AppResult<String> {
    let key = state.session().active_key()?;
    let plaintext = crypto::decrypt(&key, &sealed.nonce, &sealed.ciphertext)?;
    let Some(template) = template else {
        return Ok(plaintext.to_string());
    };
    let spans: Zeroizing<Vec<String>> = Zeroizing::new(
        serde_json::from_str(&plaintext)
            .map_err(|_| AppError::Crypto("Corrupted secrets".to_string()))?,
    );
    Ok(secrets::restore(template, &spans))
}
