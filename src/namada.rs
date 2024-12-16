use std::{collections::HashSet, ops::Add, str::FromStr};

use anyhow::Context;
use futures::{FutureExt, StreamExt};
use namada_sdk::{
    address::Address, dec::Dec, key::common::SecretKey, queries::RPC, rpc, state::Epoch, token,
};
use tendermint_rpc::HttpClient;

use crate::utils;

pub trait NamadaRpc {
    async fn get_current_epoch(&self) -> anyhow::Result<Epoch>;
    async fn get_pos_inflation_rate(&self) -> anyhow::Result<f64>;
    async fn get_delegators_validators(
        &self,
        address: &Address,
        epoch: u64,
    ) -> anyhow::Result<HashSet<Address>>;
    async fn query_native_token(&self) -> anyhow::Result<Address>;
    async fn query_pos_rewards(
        &self,
        validators: HashSet<Address>,
        delegator_address: &Address,
    ) -> anyhow::Result<f64>;
    async fn query_bond(
        &self,
        validator: &Address,
        delegator: &Address,
        epoch: u64,
    ) -> anyhow::Result<token::Amount>;
    async fn query_balance(
        &self,
        address: &Address,
        native_token_address: &Address,
    ) -> anyhow::Result<token::Amount>;
    async fn claim_rewards(
        &self,
        delegator_address: &Address,
        validators: HashSet<Address>,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()>;
    async fn bond(
        &self,
        delegator_address: &Address,
        validators: HashSet<Address>,
        amount: token::Amount,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()>;
    async fn query_validator_commissions(
        &self,
        validator: &Address,
        epoch: u64,
    ) -> anyhow::Result<f64>;
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
        utils::dec_to_f64(pos_inflation.inflation_rate)
    }

    async fn get_delegators_validators(
        &self,
        address: &Address,
        epoch: u64,
    ) -> anyhow::Result<HashSet<Address>> {
        let epoch = utils::to_namada_epoch(epoch);
        let index_set = rpc::get_delegation_validators(&self.client, address, epoch)
            .await
            .context("Failed fetching validators")?;
        Ok(index_set
            .into_iter()
            .map(|address| address)
            .collect::<HashSet<_>>())
    }

    async fn query_pos_rewards(
        &self,
        validators: HashSet<Address>,
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
                            &validator_address,
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
            .map(|amount| utils::amount_to_f64(amount))
            .await
            .context("Error fetching bonds")
    }

    async fn get_current_epoch(&self) -> anyhow::Result<Epoch> {
        rpc::query_epoch(&self.client).await.context("Error fetching epoch")
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
        validators: HashSet<Address>,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn bond(
        &self,
        delegator_address: &Address,
        validators: HashSet<Address>,
        amount: token::Amount,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn query_validator_commissions(
        &self,
        validator: &Address,
        epoch: u64,
    ) -> anyhow::Result<f64> {
        let epoch = utils::to_namada_epoch(epoch);
        let commission = rpc::query_commission_rate(&self.client, validator, Some(epoch))
            .await
            .context("Error fetching validator commissions")?;
        utils::dec_to_f64(commission.commission_rate.unwrap())
    }

    async fn query_bond(
        &self,
        validator: &Address,
        delegator: &Address,
        epoch: u64,
    ) -> anyhow::Result<token::Amount> {
        let epoch = utils::to_namada_epoch(epoch);
        rpc::query_bond(&self.client, delegator, validator, Some(epoch))
            .await
            .context("Error fetching bonds")
    }

    async fn query_native_token(&self) -> anyhow::Result<Address> {
        rpc::query_native_token(&self.client)
            .await
            .context("Error fetching native token")
    }
}

#[derive(Debug, Clone)]
pub struct TestNamadaSdk {
    client: HttpClient,
}

impl NamadaRpc for TestNamadaSdk {
    async fn get_pos_inflation_rate(&self) -> anyhow::Result<f64> {
        Ok(Dec::new(1, 2).unwrap().to_string().parse::<f64>().unwrap())
    }

    async fn get_delegators_validators(
        &self,
        address: &Address,
        epoch: u64,
    ) -> anyhow::Result<HashSet<Address>> {
        Ok(HashSet::from_iter([Address::from_str(
            "tnam1qyff9lf3hsmlmj67lyqvxn9xez7a6utlhumk8vlv",
        )
        .unwrap()]))
    }

    async fn query_pos_rewards(
        &self,
        validators: HashSet<Address>,
        delegator_address: &Address,
    ) -> anyhow::Result<f64> {
        todo!()
    }

    async fn get_current_epoch(&self) -> anyhow::Result<Epoch> {
        Ok(Epoch(1))
    }

    async fn query_balance(
        &self,
        address: &Address,
        native_token_address: &Address,
    ) -> anyhow::Result<token::Amount> {
        todo!()
    }

    async fn claim_rewards(
        &self,
        delegator_address: &Address,
        validators: HashSet<Address>,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()> {
        todo!()
    }

    async fn bond(
        &self,
        delegator_address: &Address,
        validators: HashSet<Address>,
        amount: token::Amount,
        secret_key: &SecretKey,
    ) -> anyhow::Result<()> {
        todo!()
    }

    async fn query_validator_commissions(
        &self,
        validator: &Address,
        epoch: u64,
    ) -> anyhow::Result<f64> {
        todo!()
    }

    async fn query_bond(
        &self,
        validator: &Address,
        delegator: &Address,
        epoch: u64,
    ) -> anyhow::Result<token::Amount> {
        todo!()
    }

    async fn query_native_token(&self) -> anyhow::Result<Address> {
        todo!()
    }
}
