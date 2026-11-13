//! The legacy Ditto/KeePass-style AES envelope. It is confined to this adapter.
use aes::{
    Aes256,
    cipher::{
        BlockDecryptMut, BlockEncrypt, BlockEncryptMut, KeyInit, KeyIvInit, block_padding::Pkcs7,
    },
};
use anyhow::{Result, ensure};
use rand::RngCore;
use sha2::{Digest, Sha256};
const HEADER: usize = 140;
const ROUNDS: u32 = 100_000;
fn key(password: &str, header: &[u8]) -> [u8; 32] {
    let mut transformed: [u8; 32] = Sha256::digest(password.as_bytes()).into();
    let aes = Aes256::new_from_slice(&header[104..136]).expect("fixed key slice");
    for _ in 0..ROUNDS {
        for block in transformed.as_chunks_mut::<16>().0 {
            aes.encrypt_block(block.into());
        }
    }
    let hashed = Sha256::digest(transformed);
    let mut final_key = Sha256::new();
    final_key.update(&header[40..56]);
    final_key.update(hashed);
    final_key.finalize().into()
}
pub fn encrypt(input: &[u8], password: &str) -> Result<Vec<u8>> {
    ensure!(
        !password.is_empty() && !input.is_empty(),
        "Empty Ditto block or password"
    );
    let mut h = vec![0; HEADER];
    h[32..36].copy_from_slice(&0x139C5AFEu32.to_le_bytes());
    h[36..40].copy_from_slice(&0xBF3562DAu32.to_le_bytes());
    rand::rng().fill_bytes(&mut h[40..72]);
    rand::rng().fill_bytes(&mut h[104..136]);
    h[72..104].copy_from_slice(&Sha256::digest(input));
    h[136..140].copy_from_slice(&ROUNDS.to_le_bytes());
    let hash = Sha256::digest(&h[32..]);
    h[..32].copy_from_slice(&hash);
    let cipher = cbc::Encryptor::<Aes256>::new((&key(password, &h)).into(), h[56..72].into());
    let bytes = cipher.encrypt_padded_vec_mut::<Pkcs7>(input);
    h.extend(bytes);
    Ok(h)
}
pub fn decrypt(input: &[u8], password: &str) -> Result<Vec<u8>> {
    ensure!(
        input.len() >= HEADER + 16 && (input.len() - HEADER).is_multiple_of(16),
        "Invalid Ditto envelope length"
    );
    let h = &input[..HEADER];
    ensure!(
        h[32..36] == 0x139C5AFEu32.to_le_bytes() && h[36..40] == 0xBF3562DAu32.to_le_bytes(),
        "Invalid Ditto signatures"
    );
    ensure!(
        h[136..140] == ROUNDS.to_le_bytes(),
        "Unsupported Ditto KDF rounds"
    );
    ensure!(
        h[..32] == Sha256::digest(&h[32..])[..],
        "Invalid Ditto header hash"
    );
    let cipher = cbc::Decryptor::<Aes256>::new((&key(password, h)).into(), h[56..72].into());
    let plain = cipher
        .decrypt_padded_vec_mut::<Pkcs7>(&input[HEADER..])
        .map_err(|_| anyhow::anyhow!("Invalid Ditto password or payload"))?;
    ensure!(
        h[72..104] == Sha256::digest(&plain)[..],
        "Invalid Ditto contents hash"
    );
    Ok(plain)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_size_and_password_are_verified() {
        let wire = encrypt(&vec![0; 360], "password").unwrap();
        assert_eq!(wire.len(), 508);
        assert_eq!(decrypt(&wire, "password").unwrap(), vec![0; 360]);
        assert!(decrypt(&wire, "wrong").is_err());
        let mut corrupt = wire;
        corrupt[45] ^= 1;
        assert!(decrypt(&corrupt, "password").is_err());
    }
}
