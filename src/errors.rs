use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    DeviceNotFound,
    DeviceUnreachable,
    NetworkError,
    AuthRequired,
    GroupNotFound,
    NoCachedDevices,
    PartialFailure,
}

impl ErrorCode {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::InvalidInput => 1,
            Self::DeviceNotFound | Self::GroupNotFound | Self::NoCachedDevices => 1,
            Self::DeviceUnreachable | Self::NetworkError => 2,
            Self::AuthRequired => 3,
            Self::PartialFailure => 4,
        }
    }
}

/// Classify an anyhow error into a structured `CliError` by inspecting the message.
pub fn classify_error(err: &anyhow::Error) -> CliError {
    let message = format!("{err:#}");
    let lower = message.to_lowercase();

    let code = if lower.contains("not found in cache") || lower.contains("did you mean") {
        ErrorCode::DeviceNotFound
    } else if lower.contains("no cached devices") {
        ErrorCode::NoCachedDevices
    } else if lower.contains("group") && lower.contains("not found") {
        ErrorCode::GroupNotFound
    } else if lower.contains("auth") || lower.contains("unauthorized") || lower.contains("401") {
        ErrorCode::AuthRequired
    } else if lower.contains("timed out") || lower.contains("connect") {
        ErrorCode::DeviceUnreachable
    } else if lower.contains("partial") || lower.contains("some devices") {
        ErrorCode::PartialFailure
    } else if lower.contains("invalid") || lower.contains("parse") || lower.contains("specify") {
        ErrorCode::InvalidInput
    } else {
        ErrorCode::NetworkError
    };

    CliError { code, message }
}
