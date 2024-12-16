use std::{str::FromStr, time::Duration};

use anyhow::Context;
use clap::Parser;
use config::AppConfig;
use futures::{FutureExt, StreamExt};
use namada::{NamadaRpc, NamadaSdk};
use namada_sdk::{address::Address, key::common::SecretKey, token};
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

    let client = HttpClient::new(config.namada_rpc.as_str()).unwrap();
    let namada_sdk = NamadaSdk::new(client);

    loop {
        let current_epoch = namada_sdk.get_current_epoch().await.unwrap().0;

        let pos_inflation = namada_sdk.get_pos_inflation_rate().await.unwrap();

        tracing::info!("Inflation rate is: {}", pos_inflation);

        let secret_key = SecretKey::from_str(&config.secret_key).unwrap();
        let public_key = secret_key.to_public();
        let delegator_address = Address::from(&public_key);

        tracing::info!("Delegator address is: {}", delegator_address);

        let validators = namada_sdk
            .get_delegators_validators(&delegator_address, current_epoch)
            .await
            .unwrap();

        let commissions = futures::stream::iter(&validators)
            .map(|address| {
                let sdk_clone = namada_sdk.clone();
                async move {
                    sdk_clone
                        .query_validator_commissions(address, current_epoch)
                        .await
                        .unwrap_or_default()
                }
            })
            .buffer_unordered(20)
            .collect::<Vec<_>>()
            .await;

        let mean_commissions = utils::mean(&commissions).unwrap();

        let amount_bonded = futures::stream::iter(&validators)
            .map(|validator_address| {
                let sdk_clone = namada_sdk.clone();
                let delegator_address_clone = delegator_address.clone();
                async move {
                    sdk_clone
                        .query_bond(validator_address, &delegator_address_clone, current_epoch)
                        .await
                        .unwrap_or_default()
                }
            })
            .buffer_unordered(20)
            .fold(token::Amount::zero(), |acc, amount| async move {
                acc.checked_add(amount).unwrap()
            })
            .map(|amount| utils::amount_to_f64(amount))
            .await
            .context("Error fetching bonds")?;

        let net_apr = pos_inflation - (pos_inflation * mean_commissions);

        let optimization_result = opt::compute_frequency_opt(
            amount_bonded,
            net_apr,
            config.base_fee_unam * validators.len() as f64,
        )
        .unwrap();

        if config.dry_run {
            tracing::info!("Dry-run mode");
            tracing::info!(
                "- Compunding frequency: {:.2} hours / {:.2} days",
                optimization_result.hours_between_compounding_rounded(),
                optimization_result.days_between_compounding_rounded()
            );
            tracing::info!("- Current bonded balance: {:.2}", amount_bonded);
            tracing::info!(
                "- Balance in 1 year: {:.2}",
                optimization_result.max_balance
            );
            tracing::info!("- APR: {:.2}%", net_apr * 100.0);
            tracing::info!(
                "- APY: {:.2}%",
                ((optimization_result.max_balance / amount_bonded) - 1.0) * 100.0
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

        let native_token_address = namada_sdk.query_native_token().await.unwrap();

        let balance_pre = namada_sdk
            .query_balance(&delegator_address, &native_token_address)
            .await
            .unwrap();

        tracing::info!("Pre balance: {}", balance_pre.to_string_native());

        namada_sdk
            .claim_rewards(&delegator_address, validators.clone(), &secret_key)
            .await
            .unwrap();

        let balance_post = namada_sdk
            .query_balance(&delegator_address, &native_token_address)
            .await
            .unwrap();

        tracing::info!("Post balance: {}", balance_post.to_string_native());

        let rewards = balance_post.checked_sub(balance_pre).unwrap();

        namada_sdk
            .bond(&delegator_address, validators, rewards, &secret_key)
            .await
            .unwrap();

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
