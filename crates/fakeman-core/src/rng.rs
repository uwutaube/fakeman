//! Randomness and clock used while generating values.

use chrono::{DateTime, Utc};
use rand::SeedableRng;
use rand::rngs::StdRng;

/// The source of randomness for one generation pass, plus the reference time that
/// relative date generators (`date.past`, `date.future`) count from.
///
/// A context created with [`GenContext::seeded`] and a fixed time via [`GenContext::at`]
/// produces exactly the same values every time.
pub struct GenContext {
    pub rng: StdRng,
    pub now: DateTime<Utc>,
}

impl GenContext {
    /// Seeds from the operating system and uses the current time.
    pub fn new() -> Self {
        Self {
            rng: rand::make_rng(),
            now: Utc::now(),
        }
    }

    /// Deterministic randomness from `seed`, using the current time.
    pub fn seeded(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            now: Utc::now(),
        }
    }

    /// Replaces the reference time.
    pub fn at(mut self, now: DateTime<Utc>) -> Self {
        self.now = now;
        self
    }
}

impl Default for GenContext {
    fn default() -> Self {
        Self::new()
    }
}
