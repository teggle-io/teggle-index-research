use std::fmt::{Debug};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: String) -> Error {
        Error { message }
    }

    pub fn into_string(self) -> String {
        self.into()
    }
}

impl AsRef<str> for Error {
    fn as_ref(&self) -> &str {
        &self.message
    }
}

impl From<Error> for String {
    fn from(e: Error) -> String {
        e.message
    }
}

/// Access to the node's backend db
pub trait Db
    where
        Self: 'static,
{
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;

    fn delete(&self, key: &[u8]) -> Result<()>;

    fn flush(&self) -> Result<()>;
}
