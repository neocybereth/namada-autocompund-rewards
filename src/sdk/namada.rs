use std::{path::PathBuf, str::FromStr};

use namada_sdk::{
    address::{Address, ImplicitAddress},
    args::TxBuilder,
    chain::ChainId,
    io::NullIo,
    key::common::{PublicKey, SecretKey},
    masp::fs::FsShieldedUtils,
    rpc,
    wallet::{fs::FsWalletUtils, Wallet},
    Namada, NamadaImpl, ShieldedWallet,
};
use tendermint_rpc::HttpClient;

use crate::config::AppConfig;

pub struct Sdk {
    pub base_dir: PathBuf,
    pub chain_id: String,
    pub rpc: String,
    pub namada: NamadaImpl<HttpClient, FsWalletUtils, FsShieldedUtils, NullIo>,
}

impl Sdk {
    pub async fn new(
        config: &AppConfig,
        base_dir: &PathBuf,
        http_client: HttpClient,
        wallet: Wallet<FsWalletUtils>,
        shielded_ctx: ShieldedWallet<FsShieldedUtils>,
        io: NullIo,
    ) -> Sdk {
        // Insert the faucet keypair into the wallet
        let sk = SecretKey::from_str(&config.faucet_sk).unwrap();
        let public_key = sk.to_public();
        let address = Address::Implicit(ImplicitAddress::from(&public_key));

        let namada = NamadaImpl::new(http_client, wallet, shielded_ctx, io)
            .await
            .expect("unable to construct Namada object")
            .chain_id(ChainId::from_str(&config.chain_id).unwrap());

        let mut namada_wallet = namada.wallet.write().await;
        namada_wallet
            .insert_keypair("faucet".to_string(), true, sk, None, Some(address), None)
            .unwrap();

        let native_token = rpc::query_native_token(&namada.clone_client())
            .await
            .unwrap();
        namada_wallet
            .insert_address("nam", native_token, true)
            .unwrap();
        drop(namada_wallet);

        Self {
            base_dir: base_dir.to_owned(),
            chain_id: config.chain_id.to_owned(),
            rpc: config.rpc.to_owned(),
            namada,
        }
    }

    pub async fn find_secret_key(&self, alias: impl AsRef<str>) -> SecretKey {
        let mut wallet = self.namada.wallet.write().await;
        wallet.find_secret_key(alias, None).unwrap()
    }

    pub async fn find_public_key(&self, alias_or_pkh: impl AsRef<str>) -> PublicKey {
        let wallet = self.namada.wallet.write().await;
        wallet.find_public_key(alias_or_pkh).unwrap()
    }
}