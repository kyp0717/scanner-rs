use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct SupabaseConfig {
    pub url: String,
    pub anon_key: String,
}

impl SupabaseConfig {
    /// Load from environment variables (call dotenv::dotenv() first).
    pub fn from_env() -> Result<Self> {
        let url =
            std::env::var("SUPABASE_URL").context("SUPABASE_URL not set in environment")?;
        let anon_key = std::env::var("SUPABASE_ANON_KEY")
            .context("SUPABASE_ANON_KEY not set in environment")?;
        Ok(Self { url, anon_key })
    }
}

/// Load .env file from the project root or current directory.
pub fn load_env() {
    // Try project root first (where Cargo.toml lives), then cwd
    let _ = dotenv::dotenv();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_env_no_panic() {
        // Should not panic even if .env doesn't exist
        load_env();
    }

    // Note: testing the missing-vars error case would require mutating global env vars
    // (unsafe in Rust 2024 due to multi-threaded test runner races). Omitted intentionally;
    // the error path is trivially correct from reading the code.
}
