use thiserror::Error as ThisError;

/// Typed error for every `shelly-core` transport path.
///
/// The five variants map 1:1 onto `switchkit::Error`: a caller that already
/// speaks that vocabulary can translate this enum without guesswork.
#[derive(Debug, ThisError)]
pub enum Error {
    /// The device could not be reached at all (connect/timeout/read failure),
    /// or it responded with a non-success HTTP status that is not an auth
    /// failure.
    #[error("network error: {message}")]
    Network { message: String },

    /// The device rejected the request because of missing or invalid
    /// credentials (HTTP 401/403).
    #[error("authentication error: {message}")]
    Auth { message: String },

    /// The device was reached, answered with a well-formed response, but
    /// explicitly rejected the request (a Gen2 RPC `error` object in an
    /// HTTP 200 body).
    #[error("request rejected: {message}")]
    Rejected { message: String },

    /// The device was reached but its response could not be interpreted:
    /// invalid JSON, or a `/shelly` body that does not describe a Shelly
    /// device. Reachable-but-not-Shelly is deliberately `Parse`, not
    /// `Network`, so callers can distinguish "nothing there" from "something
    /// else is there".
    #[error("failed to parse response: {message}")]
    Parse { message: String },

    /// The operation is genuinely not supported, either by this device
    /// generation or by this client (replaces the old `anyhow::bail!("not
    /// supported")` paths).
    #[error("unsupported operation: {message}")]
    Unsupported { message: String },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Classify a raw `reqwest::Error` into a typed `Error`.
///
/// `reqwest::Error` already distinguishes response-decode failures
/// (`is_decode`) from connect/timeout/request-level failures, so a plain `?`
/// on `send()`/`json()` calls routes through this impl and lands on the
/// right variant without every call site repeating the classification.
impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        if err.is_decode() {
            Error::Parse {
                message: scrub(&err.to_string()),
            }
        } else {
            Error::Network {
                message: scrub(&err.to_string()),
            }
        }
    }
}

/// Classify a non-success HTTP response into `Auth` (401/403) or `Network`
/// (everything else). An HTTP 200 with an RPC-error body is handled
/// separately by the Gen2 RPC caller; this only covers the status line.
pub(crate) fn status_error(status: reqwest::StatusCode, url: &str, body: &str) -> Error {
    let message = scrub(&format!("HTTP {status} from {url}: {body}"));
    if status.as_u16() == 401 || status.as_u16() == 403 {
        Error::Auth { message }
    } else {
        Error::Network { message }
    }
}

/// Scrub potentially sensitive data from a raw error/diagnostic string
/// before it becomes part of a typed `Error`.
///
/// Shelly authentication is sent as a Basic-auth HTTP header, never embedded
/// in a URL, so the leak surface here is small today. This still strips a
/// `user:pass@` userinfo component defensively (e.g. if a future caller ever
/// builds a URL that way), and exists as a single choke point so every
/// transport path scrubs uniformly instead of ad hoc per call site.
pub(crate) fn scrub(raw: &str) -> String {
    match (raw.find("://"), raw.find('@')) {
        (Some(scheme_end), Some(at)) if at > scheme_end + 3 => {
            let mut out = String::with_capacity(raw.len());
            out.push_str(&raw[..scheme_end + 3]);
            out.push_str(&raw[at + 1..]);
            out
        }
        _ => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_strips_userinfo_from_url() {
        let raw = "error sending request for url (http://admin:secret@192.0.2.1/status)";
        let scrubbed = scrub(raw);
        assert!(!scrubbed.contains("secret"));
        assert!(scrubbed.contains("192.0.2.1/status"));
    }

    #[test]
    fn scrub_leaves_plain_messages_untouched() {
        let raw = "connection refused";
        assert_eq!(scrub(raw), raw);
    }

    #[test]
    fn status_error_401_is_auth() {
        let err = status_error(
            reqwest::StatusCode::UNAUTHORIZED,
            "http://192.0.2.1/status",
            "",
        );
        assert!(matches!(err, Error::Auth { .. }));
    }

    #[test]
    fn status_error_403_is_auth() {
        let err = status_error(
            reqwest::StatusCode::FORBIDDEN,
            "http://192.0.2.1/status",
            "",
        );
        assert!(matches!(err, Error::Auth { .. }));
    }

    #[test]
    fn status_error_500_is_network() {
        let err = status_error(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "http://192.0.2.1/status",
            "",
        );
        assert!(matches!(err, Error::Network { .. }));
    }
}
