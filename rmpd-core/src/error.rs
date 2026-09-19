use thiserror::Error;

#[derive(Error, Debug)]
pub enum RmpdError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Player error: {0}")]
    Player(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Library error: {0}")]
    Library(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Permission denied")]
    PermissionDenied,
}

pub type Result<T> = std::result::Result<T, RmpdError>;

impl RmpdError {
    /// Return the error detail without category prefixes from `Display`.
    #[must_use]
    pub fn detail_message(&self) -> std::borrow::Cow<'_, str> {
        use std::borrow::Cow;

        match self {
            Self::Config(msg)
            | Self::Database(msg)
            | Self::Player(msg)
            | Self::Protocol(msg)
            | Self::ParseError(msg)
            | Self::Library(msg)
            | Self::Storage(msg)
            | Self::NotFound(msg)
            | Self::InvalidState(msg) => Cow::Borrowed(msg.as_str()),
            Self::Io(err) => Cow::Owned(err.to_string()),
            Self::PermissionDenied => Cow::Borrowed("Permission denied"),
        }
    }
}

// Automatic error conversions for common dependency errors
#[cfg(feature = "database-errors")]
impl From<rusqlite::Error> for RmpdError {
    fn from(err: rusqlite::Error) -> Self {
        RmpdError::Database(err.to_string())
    }
}

#[cfg(feature = "player-errors")]
impl From<symphonia::core::errors::Error> for RmpdError {
    fn from(err: symphonia::core::errors::Error) -> Self {
        RmpdError::Player(err.to_string())
    }
}

#[cfg(feature = "player-errors")]
impl From<cpal::Error> for RmpdError {
    fn from(err: cpal::Error) -> Self {
        RmpdError::Player(err.to_string())
    }
}

#[cfg(feature = "library-errors")]
impl From<lofty::error::FileParseError> for RmpdError {
    fn from(err: lofty::error::FileParseError) -> Self {
        RmpdError::Library(err.to_string())
    }
}

#[cfg(feature = "library-errors")]
impl From<lofty::error::TagParseError> for RmpdError {
    fn from(err: lofty::error::TagParseError) -> Self {
        RmpdError::Library(err.to_string())
    }
}

#[cfg(feature = "library-errors")]
impl From<tantivy::TantivyError> for RmpdError {
    fn from(err: tantivy::TantivyError) -> Self {
        RmpdError::Library(err.to_string())
    }
}

#[cfg(feature = "library-errors")]
impl From<notify::Error> for RmpdError {
    fn from(err: notify::Error) -> Self {
        RmpdError::Library(err.to_string())
    }
}

#[cfg(feature = "protocol-errors")]
impl From<mdns_sd::Error> for RmpdError {
    fn from(err: mdns_sd::Error) -> Self {
        RmpdError::Protocol(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::RmpdError;
    use std::borrow::Cow;

    #[test]
    fn detail_message_for_string_variants_is_borrowed() {
        let cases = vec![
            RmpdError::Config("cfg".to_owned()),
            RmpdError::Database("db".to_owned()),
            RmpdError::Player("player".to_owned()),
            RmpdError::Protocol("proto".to_owned()),
            RmpdError::ParseError("parse".to_owned()),
            RmpdError::Library("lib".to_owned()),
            RmpdError::Storage("storage".to_owned()),
            RmpdError::NotFound("missing".to_owned()),
            RmpdError::InvalidState("bad-state".to_owned()),
        ];

        for err in cases {
            assert!(matches!(err.detail_message(), Cow::Borrowed(_)));
        }
    }

    #[test]
    fn detail_message_for_io_is_owned_and_preserves_text() {
        let io = std::io::Error::other("disk offline");
        let err = RmpdError::Io(io);

        let detail = err.detail_message();
        assert!(matches!(detail, Cow::Owned(_)));
        assert_eq!(detail, "disk offline");
    }

    #[test]
    fn permission_denied_detail_message_is_stable() {
        let err = RmpdError::PermissionDenied;
        assert_eq!(err.detail_message(), "Permission denied");
    }

    #[test]
    fn display_prefixes_match_error_kind() {
        assert_eq!(
            RmpdError::Config("broken".to_owned()).to_string(),
            "Configuration error: broken"
        );
        assert_eq!(
            RmpdError::Protocol("bad command".to_owned()).to_string(),
            "Protocol error: bad command"
        );
        assert_eq!(RmpdError::PermissionDenied.to_string(), "Permission denied");
    }

    #[cfg(feature = "database-errors")]
    #[test]
    fn conversion_impl_exists_for_rusqlite_error() {
        fn assert_into<E: Into<RmpdError>>() {}
        assert_into::<rusqlite::Error>();
    }

    #[cfg(feature = "player-errors")]
    #[test]
    fn conversion_impl_exists_for_player_errors() {
        fn assert_into<E: Into<RmpdError>>() {}
        assert_into::<symphonia::core::errors::Error>();
        assert_into::<cpal::Error>();
    }

    #[cfg(feature = "library-errors")]
    #[test]
    fn conversion_impl_exists_for_library_errors() {
        fn assert_into<E: Into<RmpdError>>() {}
        assert_into::<lofty::error::FileParseError>();
        assert_into::<lofty::error::TagParseError>();
        assert_into::<tantivy::TantivyError>();
        assert_into::<notify::Error>();
    }

    #[cfg(feature = "protocol-errors")]
    #[test]
    fn conversion_impl_exists_for_mdns_error() {
        fn assert_into<E: Into<RmpdError>>() {}
        assert_into::<mdns_sd::Error>();
    }
}
