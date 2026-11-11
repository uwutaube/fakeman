//! Domain model, generator catalog, body builder and runner. Filled in from Phase 1.

pub const NAME: &str = "fakeman-core";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name() {
        assert_eq!(super::NAME, env!("CARGO_PKG_NAME"));
    }
}
