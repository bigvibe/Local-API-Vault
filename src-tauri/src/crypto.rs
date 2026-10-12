use age::secrecy::{ExposeSecret, SecretString};
use std::io::Read;
use thiserror::Error;
use zeroize::Zeroize;

const MAX_SCRYPT_WORK_FACTOR: u8 = 22;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed")]
    Decrypt,
    #[error("invalid identity data")]
    InvalidIdentity,
}

pub fn encrypt(plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>, CryptoError> {
    let secret = SecretString::from(passphrase.to_owned());
    let recipient = age::scrypt::Recipient::new(secret);
    age::encrypt(&recipient, plaintext).map_err(|_| CryptoError::Encrypt)
}

pub fn encrypt_to_identity(
    plaintext: &[u8],
    identity: &age::x25519::Identity,
) -> Result<Vec<u8>, CryptoError> {
    age::encrypt(&identity.to_public(), plaintext).map_err(|_| CryptoError::Encrypt)
}

pub fn encrypt_identity_sidecar(
    identity: &age::x25519::Identity,
    passphrase: &str,
) -> Result<Vec<u8>, CryptoError> {
    let secret = identity.to_string();
    encrypt(secret.expose_secret().as_bytes(), passphrase)
}

pub fn decrypt(ciphertext: &[u8], passphrase: &str) -> Result<Vec<u8>, CryptoError> {
    let secret = SecretString::from(passphrase.to_owned());
    let mut identity = age::scrypt::Identity::new(secret);
    identity.set_max_work_factor(MAX_SCRYPT_WORK_FACTOR);
    age::decrypt(&identity, ciphertext).map_err(|_| CryptoError::Decrypt)
}

pub fn is_scrypt_encrypted(ciphertext: &[u8]) -> bool {
    age::Decryptor::new_buffered(ciphertext)
        .map(|decryptor| decryptor.is_scrypt())
        .unwrap_or(false)
}

pub fn decrypt_identity_sidecar(
    ciphertext: &[u8],
    passphrase: &str,
) -> Result<age::x25519::Identity, CryptoError> {
    let mut plaintext = decrypt(ciphertext, passphrase)?;
    let result = std::str::from_utf8(&plaintext)
        .map_err(|_| CryptoError::InvalidIdentity)
        .and_then(|text| {
            text.trim()
                .parse::<age::x25519::Identity>()
                .map_err(|_| CryptoError::InvalidIdentity)
        });
    plaintext.zeroize();
    result
}

pub fn decrypt_with_identity(
    ciphertext: &[u8],
    identity: &age::x25519::Identity,
) -> Result<Vec<u8>, CryptoError> {
    let decryptor = age::Decryptor::new_buffered(ciphertext).map_err(|_| CryptoError::Decrypt)?;
    let mut reader = decryptor
        .decrypt(std::iter::once(identity as &dyn age::Identity))
        .map_err(|_| CryptoError::Decrypt)?;
    let mut plaintext = Vec::new();
    reader
        .read_to_end(&mut plaintext)
        .map_err(|_| CryptoError::Decrypt)?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrase_round_trip() {
        let plaintext = b"a local secret that should not be logged";
        let ciphertext = encrypt(plaintext, "correct horse battery staple").unwrap();
        assert_ne!(ciphertext, plaintext);
        let decrypted = decrypt(&ciphertext, "correct horse battery staple").unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn identity_sidecar_round_trip() {
        let identity = age::x25519::Identity::generate();
        let sidecar = encrypt_identity_sidecar(&identity, "correct horse battery staple").unwrap();
        let recovered = decrypt_identity_sidecar(&sidecar, "correct horse battery staple").unwrap();
        assert_eq!(
            recovered.to_public().to_string(),
            identity.to_public().to_string()
        );
    }

    #[test]
    fn recovery_identity_round_trip() {
        let identity = age::x25519::Identity::generate();
        let plaintext = b"recoverable secret";
        let ciphertext = encrypt_to_identity(plaintext, &identity).unwrap();

        assert_eq!(
            decrypt_with_identity(&ciphertext, &identity).unwrap(),
            plaintext
        );
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let ciphertext = encrypt(b"secret", "correct horse battery staple").unwrap();
        assert!(decrypt(&ciphertext, "not the right password").is_err());
    }
}
