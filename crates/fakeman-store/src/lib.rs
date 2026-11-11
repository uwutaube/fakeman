//! SQLite schema, migrations and repositories. Filled in from Phase 2.

pub const NAME: &str = "fakeman-store";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name() {
        assert_eq!(super::NAME, env!("CARGO_PKG_NAME"));
    }
}
