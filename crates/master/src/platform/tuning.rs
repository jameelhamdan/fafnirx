//! Sizes that can be set from the environment to fit the master to the cores it is given (see `main.rs`).

/// The positive number in the variable `var`, or `None` when it is not set, empty or not a positive number.
pub fn get(var: &str) -> Option<usize> {
    let value = std::env::var(var).ok()?;
    match value.trim().parse::<usize>() {
        Ok(n) if n > 0 => Some(n),
        _ if value.trim().is_empty() => None,
        _ => {
            tracing::warn!("{var}={value:?} is not a positive number and is ignored");
            None
        }
    }
}
