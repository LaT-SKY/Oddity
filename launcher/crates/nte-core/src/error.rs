use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Http(String),
    Parse(String),
    Integrity(String),
    Unsupported(String),
    Cancelled,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Http(m) => write!(f, "http: {m}"),
            Error::Parse(m) => write!(f, "parse: {m}"),
            Error::Integrity(m) => write!(f, "integrity: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
            Error::Cancelled => write!(f, "任务已取消"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_prefixes_every_variant() {
        assert_eq!(Error::Http("boom".into()).to_string(), "http: boom");
        assert_eq!(Error::Parse("bad".into()).to_string(), "parse: bad");
        assert_eq!(Error::Integrity("sum".into()).to_string(), "integrity: sum");
        assert_eq!(
            Error::Unsupported("nope".into()).to_string(),
            "unsupported: nope"
        );
        let io = Error::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing file",
        ));
        assert_eq!(io.to_string(), "io: missing file");
    }

    #[test]
    fn error_is_a_std_error_without_a_source() {
        let err: Box<dyn std::error::Error> = Box::new(Error::Unsupported("x".into()));
        assert!(err.source().is_none());
        assert_eq!(err.to_string(), "unsupported: x");
        let ok: Result<u32> = Ok(7);
        assert_eq!(ok.ok(), Some(7));
    }
}
