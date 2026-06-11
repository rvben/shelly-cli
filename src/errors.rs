use std::io::IsTerminal;

/// Sentinel error type for commands that require explicit confirmation.
///
/// When a destructive command runs without a TTY and without --yes, this error
/// is returned. The classifier recognises it by downcasting, emits the
/// `confirmation_required` kind, and exits with code 2.
#[derive(Debug)]
pub struct ConfirmationRequired {
    pub resource: String,
}

impl std::fmt::Display for ConfirmationRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Deleting {} requires confirmation", self.resource)
    }
}

impl std::error::Error for ConfirmationRequired {}

/// Check whether the caller passed `--yes` or is running interactively.
///
/// Destructive commands call this before any network side-effects. Without a TTY
/// and without `--yes` the function returns a `ConfirmationRequired` error, which
/// `classify_error` maps to the `confirmation_required` kind (exit 2).
pub fn check_confirmation(yes: bool, resource: &str) -> anyhow::Result<()> {
    if yes || std::io::stdin().is_terminal() {
        return Ok(());
    }
    Err(ConfirmationRequired {
        resource: resource.to_string(),
    }
    .into())
}

/// Structured error emitted on stderr and used for non-zero exits.
#[derive(Debug, Clone)]
pub struct CliError {
    /// Stable snake_case identifier declared in the schema's `errors` array.
    pub kind: &'static str,
    /// Human-readable description of what went wrong.
    pub message: String,
    /// Optional actionable remediation message.
    pub hint: Option<String>,
    /// Process exit code mapped from the kind.
    pub exit_code: i32,
}

/// Classify an anyhow error into a structured `CliError`.
///
/// Checks for sentinel types first (via downcasting), then falls back to
/// string-matching on the error message.
pub fn classify_error(err: &anyhow::Error) -> CliError {
    // Sentinel: confirmation_required (exit 2)
    if let Some(cr) = err.downcast_ref::<ConfirmationRequired>() {
        return CliError {
            kind: "confirmation_required",
            message: format!("Running {} requires confirmation", cr.resource),
            hint: Some("Re-run with --yes to confirm.".to_string()),
            exit_code: 2,
        };
    }

    let message = format!("{err:#}");
    let lower = message.to_lowercase();

    // Clap parse errors (unrecognized subcommand, missing arg, etc.)
    let (kind, exit_code, hint) = if lower.starts_with("clap_error:") {
        let cleaned = message.trim_start_matches("clap_error:").trim().to_string();
        return CliError {
            kind: "invalid_input",
            message: cleaned,
            hint: Some("Run with --help to see usage.".to_string()),
            exit_code: 2,
        };
    } else if lower.contains("not found in cache") || lower.contains("did you mean") {
        ("device_not_found", 1, None)
    } else if lower.contains("no cached devices") || lower.contains("no devices discovered") {
        (
            "no_cached_devices",
            1,
            Some("Run 'shelly discover --subnet YOUR_SUBNET/24' first.".to_string()),
        )
    } else if lower.contains("group") && lower.contains("not found") {
        ("group_not_found", 1, None)
    } else if lower.contains("auth") || lower.contains("unauthorized") || lower.contains("401") {
        (
            "auth_required",
            3,
            Some("Use --password or set [auth] password in config.toml.".to_string()),
        )
    } else if lower.contains("timed out") || lower.contains("connect") {
        ("device_unreachable", 2, None)
    } else if lower.contains("partial") || lower.contains("some devices") {
        ("partial_failure", 4, None)
    } else if lower.contains("already exists") || lower.contains("conflict") {
        ("conflict", 6, None)
    } else if lower.contains("invalid") || lower.contains("parse") || lower.contains("specify") {
        ("invalid_input", 1, None)
    } else {
        ("network_error", 2, None)
    };

    CliError {
        kind,
        message,
        hint,
        exit_code,
    }
}
