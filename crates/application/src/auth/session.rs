use super::{AuthError, random_bytes, token_from_bytes};
use sha2::{Digest, Sha256};

pub fn new_token() -> Result<String, AuthError> {
    Ok(token_from_bytes(&random_bytes::<32>()?))
}

pub fn digest(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
