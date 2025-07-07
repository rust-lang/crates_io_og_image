use std::env;
use std::env::VarError;

pub fn var(key: &str) -> Result<Option<String>, VarError> {
    match env::var(key) {
        Ok(content) => Ok(Some(content)),
        Err(VarError::NotPresent) => Ok(None),
        Err(error) => Err(error),
    }
}
