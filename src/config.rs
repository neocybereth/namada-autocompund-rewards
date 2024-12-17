#[derive(clap::Parser)]
pub struct AppConfig {
    #[clap(long, env)]
    pub namada_rpc: String,

    #[clap(long, env)]
    pub secret_key: String,

    #[clap(long, env)]
    pub dry_run: bool,

    #[clap(long, env, default_value_t = 0.05)]
    pub base_fee_unam: f64,

    #[clap(long, env)]
    pub one_time: bool,

    #[clap(long, env, default_value_t = 5)]
    pub sleep_for: u64,
}
