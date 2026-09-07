use blueoath_domain::CurrencyKind;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameError {
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
    #[error("account is unavailable")]
    AccountUnavailable,
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("insufficient resource: {0:?}")]
    InsufficientResource(CurrencyKind),
    #[error("invalid state: {0}")]
    InvalidState(&'static str),
    #[error("catalog is unavailable")]
    CatalogUnavailable,
    #[error("unknown method: {0}")]
    UnknownMethod(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl GameError {
    pub const CLIENT_ERROR: i32 = 1;

    pub fn client_code(&self) -> i32 {
        Self::CLIENT_ERROR
    }
}

#[cfg(test)]
mod tests {
    use super::GameError;

    #[test]
    fn maps_domain_errors_to_one_client_error_code() {
        assert_eq!(GameError::AccountUnavailable.client_code(), 1);
    }
}
