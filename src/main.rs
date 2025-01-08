use std::{str::FromStr, time::Duration};

use anyhow::Context;
use clap::Parser;
use config::AppConfig;
use namada::{NamadaRpc, NamadaSdk};
use namada_sdk::{address::Address, key::common::SecretKey};
use state::State;
use tendermint_rpc::HttpClient;
use tokio::time::sleep;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

pub mod config;
pub mod namada;
pub mod opt;
pub mod state;
pub mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::parse();
    let mut state = State::init();

    FmtSubscriber::builder().with_max_level(Level::INFO).init();

    tracing::info!("version: {}", env!("VERGEN_GIT_SHA").to_string());

    let client = HttpClient::new(config.namada_rpc.as_str()).context("Invalid http url")?;
    let namada_sdk = NamadaSdk::new(client);

    loop {
        let current_epoch = namada_sdk.get_current_epoch().await?;

        let pos_inflation = namada_sdk.get_pos_inflation_rate().await?;

        tracing::info!("Inflation rate is: {}", pos_inflation);

        let secret_key =
            SecretKey::from_str(&config.secret_key).context("Can't parse secret key")?;
        let public_key = secret_key.to_public();
        let delegator_address = Address::from(&public_key);

        tracing::info!("Delegator address is: {}", delegator_address);

        let validators = namada_sdk
            .get_delegators_validators(&delegator_address, current_epoch)
            .await?;

        let commissions = namada_sdk
            .query_validators_commissions(&validators, current_epoch)
            .await?;

        let mean_commissions =
            utils::mean(&commissions).context("Can't compute mean commissions")?;

        let bonded_amount = namada_sdk
            .query_bonds(&validators, &delegator_address, current_epoch)
            .await?
            .iter()
            .sum::<f64>();

        let net_apr = pos_inflation - (pos_inflation * mean_commissions);

        let optimization_result = opt::compute_frequency_opt(
            bonded_amount,
            net_apr,
            config.base_fee_unam * (validators.len() * 2) as f64,
        )
        .context("Failed optimizing frequency")?;

        if config.dry_run {
            tracing::info!("Dry-run mode");
            tracing::info!(
                "- Compunding frequency: {:.2} hours / {:.2} days",
                optimization_result.hours_between_compounding_rounded(),
                optimization_result.days_between_compounding_rounded()
            );
            tracing::info!("- Current bonded balance: {:.2}", bonded_amount);
            tracing::info!(
                "- Balance in 1 year: {:.2}",
                optimization_result.max_balance
            );
            tracing::info!("- APR: {:.2}%", net_apr * 100.0);
            tracing::info!(
                "- APY: {:.2}%",
                ((optimization_result.max_balance / bonded_amount) - 1.0) * 100.0
            );

            std::process::exit(0)
        }

        if !state.should_reclaim(optimization_result.optimal_frequency) {
            tracing::info!(
                "Next reclaim in {} hours...",
                state.next_reclaim_in(optimization_result.optimal_frequency) / 60 / 60
            );
            exit_or_continue(&config, false).await;
            continue;
        }

        let native_token_address = namada_sdk.query_native_token().await?;

        let balance_pre = namada_sdk
            .query_balance(&delegator_address, &native_token_address)
            .await?;

        tracing::info!("Pre balance: {}", balance_pre.to_string_native());

        namada_sdk
            .claim_rewards(&delegator_address, &validators, &secret_key)
            .await?;

        let balance_post = namada_sdk
            .query_balance(&delegator_address, &native_token_address)
            .await?;

        tracing::info!("Post balance: {}", balance_post.to_string_native());

        let rewards = balance_post.checked_sub(balance_pre).unwrap();

        namada_sdk
            .bond(&delegator_address, &validators, rewards, &secret_key)
            .await?;

        state.update();

        exit_or_continue(&config, false).await
    }
}

pub async fn exit_or_continue(config: &AppConfig, with_error: bool) {
    if config.one_time {
        let exit_code = if with_error { 1 } else { 0 };
        std::process::exit(exit_code)
    } else {
        sleep(Duration::from_secs(config.sleep_for)).await;
    }
}
