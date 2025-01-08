use anyhow::Context;
use futures::{FutureExt, StreamExt};
use namada_sdk::{
    account::Address,
    dec::Dec,
    io::NullIo,
    key::common::SecretKey,
    masp::fs::FsShieldedUtils,
    masp::ShieldedContext,
    masp::ShieldedWallet,
    queries::RPC,
    rpc,
    signing::default_sign,
    state::Epoch as NamadaEpoch,
    token::{self, Amount},
    wallet::fs::FsWalletUtils,
    Namada,
};
use std::collections::HashSet;
use tendermint_rpc::HttpClient;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TaskError {
    #[error("error waiting for timeout")]
    Timeout,
    #[error("error building tx `{0}`")]
    Build(String),
    #[error("error fetching shielded context data `{0}`")]
    ShieldedSync(String),
}

pub trait NamadaRpc {
    async fn get_current_epoch(&self) -> anyhow::Result<u64>;

    async fn get_pos_inflation_rate(&self) -> anyhow::Result<f64>;

    async fn get_delegators_validators(
        &self,
        address: &Address,
        epoch: u64,
    ) -> anyhow::Result<HashSet<Address>>;

    async fn query_native_token(&self) -> anyhow::Result<Address>;

    async fn query_pos_rewards(
        &self,
        validators: &HashSet<Address>,
        delegator_address: &Address,
    ) -> anyhow::Result<f64>;

    async fn query_bond(
        &self,
        validator: &Address,
        delegator: &Address,
        epoch: u64,
    ) -> anyhow::Result<f64>;

    async fn query_bonds(
        &self,
        validators: &HashSet<Address>,
        delegator: &Address,
        epoch: u64,
    ) -> anyhow::Result<Vec<f64>> {
        let bonds = futures::stream::iter(validators)
            .map(|validator_address| async move {
                self.query_bond(validator_address, delegator, epoch)
                    .await
                    .unwrap_or_default()
            })
            .buffer_unordered(20)
            .collect::<Vec<_>>()
            .await;

        Ok(bonds)
    }

    async fn query_balance(
        &self,
        address: &Address,
        native_token_address: &Address,
    ) -> anyhow::Result<token::Amount>;

    async fn claim_rewards(
        &self,
        delegator_address: &Address,
        validators: &HashSet<Address>,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()>;

    async fn bond(
        &self,
        delegator_address: &Address,
        validators: &HashSet<Address>,
        amount: token::Amount,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()>;

    async fn query_validator_commissions(
        &self,
        validator: &Address,
        epoch: u64,
    ) -> anyhow::Result<f64>;

    async fn query_validators_commissions(
        &self,
        validators: &HashSet<Address>,
        epoch: u64,
    ) -> anyhow::Result<Vec<f64>> {
        let commissions = futures::stream::iter(validators)
            .map(|address| async move {
                self.query_validator_commissions(address, epoch)
                    .await
                    .unwrap_or_default()
            })
            .buffer_unordered(20)
            .collect::<Vec<_>>()
            .await;

        Ok(commissions)
    }

    fn amount_to_f64(amount: token::Amount) -> anyhow::Result<f64> {
        amount
            .to_string_native()
            .parse::<f64>()
            .context("Invalid convertion from amount to f64")
    }

    fn dec_to_f64(amount: Dec) -> anyhow::Result<f64> {
        amount
            .to_string()
            .parse::<f64>()
            .context("Invalid convertion from dec to f64")
    }

    fn to_sdk_epoch(epoch: u64) -> NamadaEpoch {
        NamadaEpoch(epoch)
    }
}

#[derive(Debug, Clone)]
pub struct NamadaSdk {
    client: HttpClient,
}

impl NamadaSdk {
    pub fn new(client: HttpClient) -> Self {
        Self { client }
    }
}

impl NamadaRpc for NamadaSdk {
    async fn get_pos_inflation_rate(&self) -> anyhow::Result<f64> {
        let pos_inflation = rpc::get_staking_rewards_rate(&self.client)
            .await
            .context("Failed fetching staking rewards")?;
        Self::dec_to_f64(pos_inflation.inflation_rate)
    }

    async fn get_delegators_validators(
        &self,
        address: &Address,
        epoch: u64,
    ) -> anyhow::Result<HashSet<Address>> {
        let epoch = Self::to_sdk_epoch(epoch);
        let index_set = rpc::get_delegation_validators(&self.client, address, epoch)
            .await
            .context("Failed fetching validators")?;
        Ok(index_set.into_iter().collect::<HashSet<_>>())
    }

    async fn query_pos_rewards(
        &self,
        validators: &HashSet<Address>,
        delegator_address: &Address,
    ) -> anyhow::Result<f64> {
        futures::stream::iter(validators)
            .map(|validator_address| {
                let delegator_address_clone = delegator_address.clone();
                async move {
                    RPC.vp()
                        .pos()
                        .rewards(
                            &self.client,
                            validator_address,
                            &Some(delegator_address_clone),
                        )
                        .await
                        .unwrap_or_default()
                }
            })
            .buffer_unordered(20)
            .fold(token::Amount::zero(), |acc, amount| async move {
                acc.checked_add(amount).unwrap()
            })
            .map(Self::amount_to_f64)
            .await
            .context("Error fetching bonds")
    }

    async fn get_current_epoch(&self) -> anyhow::Result<u64> {
        rpc::query_epoch(&self.client)
            .await
            .context("Error fetching epoch")
            .map(|epoch| epoch.0)
    }

    async fn query_balance(
        &self,
        address: &Address,
        native_token_address: &Address,
    ) -> anyhow::Result<token::Amount> {
        rpc::get_token_balance(&self.client, native_token_address, address, None)
            .await
            .context("Error fetching balance")
    }

    async fn claim_rewards(
        &self,
        delegator_address: &Address,
        validators: &HashSet<Address>,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()> {
        let null_io = NullIo;
        let wallet = FsWalletUtils::new("./sdk-wallet".into());
        let shielded = ShieldedWallet::<FsShieldedUtils>::default();
        let namada = namada_sdk::NamadaImpl::new(self.client.clone(), wallet, shielded, null_io)
            .await
            .expect("Unable to initialize Namada context");
        futures::stream::iter(validators).map(|validator_address| async {
            let mut claim_rewards_tx_builder = namada.new_claim_rewards(validator_address.clone());
            claim_rewards_tx_builder.source = Some(delegator_address.clone());

            let (mut claim_reward_tx, signing_data) = claim_rewards_tx_builder
                .build(&namada)
                .await
                .map_err(|e| TaskError::Build(e.to_string()))?;

            let tx = namada
                .sign_tx_data_with_proof(
                    &mut claim_reward_tx,
                    &claim_rewards_tx_builder,
                    signing_data,
                    default_sign,
                    (),
                )
                .await?;
            // Submit transaction here
        });
        Ok(())
    }

    async fn bond(
        &self,
        delegator_address: &Address,
        validators: &HashSet<Address>,
        amount: token::Amount,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()> {
        let namada = namada_sdk::NamadaImpl::new(&self.client, None, None, None);
        let bonds = futures::stream::iter(validators).map(|validator_address| async move {
            namada
                .await?
                .new_bond(validator_address.clone(), amount, None, None);
        });

        Ok(())
    }

    async fn query_validator_commissions(
        &self,
        validator: &Address,
        epoch: u64,
    ) -> anyhow::Result<f64> {
        let epoch = Self::to_sdk_epoch(epoch);
        let commission = rpc::query_commission_rate(&self.client, validator, Some(epoch))
            .await
            .context("Error fetching validator commissions")?;
        Self::dec_to_f64(commission.commission_rate.unwrap())
    }

    async fn query_bond(
        &self,
        validator: &Address,
        delegator: &Address,
        epoch: u64,
    ) -> anyhow::Result<f64> {
        let epoch = Self::to_sdk_epoch(epoch);
        let bonded_amount = rpc::query_bond(&self.client, delegator, validator, Some(epoch))
            .await
            .context("Error fetching bonds")?;
        Self::amount_to_f64(bonded_amount)
    }

    async fn query_native_token(&self) -> anyhow::Result<Address> {
        rpc::query_native_token(&self.client)
            .await
            .context("Error fetching native token")
    }
}
