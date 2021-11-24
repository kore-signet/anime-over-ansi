use thiserror::Error;

#[derive(Error, Debug)]
pub enum AnsiError {
    #[error("missing substation alpha headers")]
    MissingSSAHeaders,
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}
