use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Http(String),
    Parse(String),
    Integrity(String),
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Http(m) => write!(f, "http: {m}"),
            Error::Parse(m) => write!(f, "parse: {m}"),
            Error::Integrity(m) => write!(f, "integrity: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self { Error::Io(e) }
}

pub type Result<T> = std::result::Result<T, Error>;
