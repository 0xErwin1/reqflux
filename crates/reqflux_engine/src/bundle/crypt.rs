//! The secrets section of a bundle: age with a scrypt passphrase recipient, ASCII armored.
//!
//! age is a small, audited format with a passphrase mode built in (scrypt work factor
//! picked for about a second on the machine that encrypts) and a maintained Rust
//! implementation, so ReqFlux designs no cryptography of its own; anyone holding the
//! passphrase can also read the section with the `age` command line tool.

use age::secrecy::SecretString;

use crate::error::BundleError;

/// The armored age encryption of `plaintext` to `passphrase`.
///
/// # Errors
///
/// [`BundleError::EmptyPassphrase`], or the encryption failed.
pub(crate) fn encrypt(passphrase: &str, plaintext: &[u8]) -> Result<String, BundleError> {
    if passphrase.is_empty() {
        return Err(BundleError::EmptyPassphrase);
    }
    let recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_owned()));
    age::encrypt_and_armor(&recipient, plaintext)
        .map_err(|error| BundleError::Encrypt(error.to_string()))
}

/// The plaintext of an armored age file encrypted to `passphrase`.
///
/// # Errors
///
/// [`BundleError::EmptyPassphrase`], or [`BundleError::Decrypt`] for a wrong passphrase or a
/// damaged section; the reason is not echoed, it could quote the input.
pub(crate) fn decrypt(passphrase: &str, armor: &str) -> Result<Vec<u8>, BundleError> {
    if passphrase.is_empty() {
        return Err(BundleError::EmptyPassphrase);
    }
    let identity = age::scrypt::Identity::new(SecretString::from(passphrase.to_owned()));
    age::decrypt(&identity, armor.as_bytes()).map_err(|_| BundleError::Decrypt)
}
