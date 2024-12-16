use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct State {
    pub last_claimed_timestamp: u64,
    pub claimed_first_time: bool,
}

impl State {
    pub fn init() -> Self {
        Self {
            last_claimed_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            claimed_first_time: false,
        }
    }

    pub fn should_reclaim(&self, compunding_frequency: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        !self.claimed_first_time || now - self.last_claimed_timestamp >= compunding_frequency
    }

    pub fn next_reclaim_in(&self, compunding_frequency: u64) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        (compunding_frequency * 60 * 60) - (now - self.last_claimed_timestamp)
    }

    pub fn update(&mut self) {
        self.claimed_first_time = true;
        self.last_claimed_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}
