use std::sync::{
    atomic::{self, AtomicBool},
    Arc,
};

use anyhow::Context;
use namada_sdk::{dec::Dec, state::Epoch, token};
use tokio::signal;

fn must_exit_handle() -> Arc<AtomicBool> {
    let handle = Arc::new(AtomicBool::new(false));
    let task_handle = Arc::clone(&handle);
    tokio::spawn(async move {
        signal::ctrl_c()
            .await
            .expect("Error receiving interrupt signal");
        task_handle.store(true, atomic::Ordering::Relaxed);
    });
    handle
}

pub fn amount_to_f64(amount: token::Amount) -> anyhow::Result<f64> {
    amount.to_string_native().parse::<f64>().context("Invalid convertion from amount to f64")
}

pub fn dec_to_f64(amount: Dec) -> anyhow::Result<f64> {
    amount.to_string().parse::<f64>().context("Invalid convertion from dec to f64")
}

pub fn to_namada_epoch(epoch: u64) -> Epoch {
    Epoch(epoch)
}

pub fn mean(vec: &[f64]) -> Option<f64> {
    if vec.is_empty() {
        return None;
    }
    let sum: f64 = vec.iter().sum();
    Some(sum / vec.len() as f64)
}
