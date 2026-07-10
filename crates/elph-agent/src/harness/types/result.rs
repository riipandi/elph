//! Harness result helpers.

/// Fallible harness operation result. Expected failures are returned as `Err` instead of thrown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

impl<T, E> Result<T, E> {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }

    pub fn unwrap(self) -> T
    where
        E: std::fmt::Debug,
    {
        match self {
            Self::Ok(value) => value,
            Self::Err(error) => panic!("called `Result::unwrap()` on an `Err` value: {error:?}"),
        }
    }

    pub fn expect(self, message: &str) -> T
    where
        E: std::fmt::Debug,
    {
        match self {
            Self::Ok(value) => value,
            Self::Err(error) => panic!("{message}: {error:?}"),
        }
    }
}

/// Standard `Result` alias used by compaction and summarization helpers.
pub type HarnessResult<T, E> = std::result::Result<T, E>;

pub fn ok<T, E>(value: T) -> Result<T, E> {
    Result::Ok(value)
}

pub fn err<T, E>(error: E) -> Result<T, E> {
    Result::Err(error)
}

/// Return the success value or panic with the failure error.
pub fn get_or_throw<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    match result {
        Result::Ok(value) => value,
        Result::Err(error) => panic!("{error}"),
    }
}

/// Return the success value or `None`.
pub fn get_or_undefined<T, E>(result: Result<T, E>) -> Option<T> {
    match result {
        Result::Ok(value) => Some(value),
        Result::Err(_) => None,
    }
}

/// Normalize unknown thrown values into displayable error messages.
pub fn to_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
