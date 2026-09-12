use argon2::{
    Argon2,
    password_hash::{
        Error as PasswordHashError, PasswordHasher, PasswordVerifier, phc::PasswordHash,
    },
};

use crate::CryptoError;

pub struct PasswordService {
    argon2: Argon2<'static>,
}

impl PasswordService {
    #[must_use]
    pub fn recommended() -> Self {
        Self {
            argon2: Argon2::default(),
        }
    }

    pub fn hash(&self, password: &str) -> Result<String, CryptoError> {
        Ok(self.argon2.hash_password(password.as_bytes())?.to_string())
    }

    pub fn verify(&self, password: &str, encoded: &str) -> Result<bool, CryptoError> {
        let parsed = PasswordHash::new(encoded).map_err(PasswordHashError::from)?;
        match self.argon2.verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(PasswordHashError::PasswordInvalid) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

impl Default for PasswordService {
    fn default() -> Self {
        Self::recommended()
    }
}
