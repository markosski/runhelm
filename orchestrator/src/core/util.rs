use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
