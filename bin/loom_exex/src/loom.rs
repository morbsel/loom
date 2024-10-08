use alloy::network::Ethereum;
use alloy::primitives::Address;
use alloy::providers::Provider;
use alloy::transports::Transport;
use eyre::OptionExt;
use reth_exex::ExExContext;
use reth_node_api::FullNodeComponents;
use reth_tracing::tracing::info;
use std::future::Future;

use debug_provider::DebugProviderExt;
use defi_actors::{loom_exex, BlockchainActors, NodeBlockActorConfig};
use defi_blockchain::Blockchain;
use loom_topology::{BroadcasterConfig, EncoderConfig, TopologyConfig};

pub async fn init<Node: FullNodeComponents>(
    ctx: ExExContext<Node>,
    bc: Blockchain,
    config: NodeBlockActorConfig,
) -> eyre::Result<impl Future<Output = eyre::Result<()>>> {
    Ok(loom_exex(ctx, bc, config.clone()))
}

pub async fn start_loom<P, T>(provider: P, bc: Blockchain, topology_config: TopologyConfig) -> eyre::Result<()>
where
    T: Transport + Clone,
    P: Provider<T, Ethereum> + DebugProviderExt<T, Ethereum> + Send + Sync + Clone + 'static,
{
    let chain_id = provider.get_chain_id().await?;

    info!(chain_id = ?chain_id, "Starting Loom" );

    let (_encoder_name, encoder) = topology_config.encoders.iter().next().ok_or_eyre("NO_ENCODER")?;

    let multicaller_address: Option<Address> = match encoder {
        EncoderConfig::SwapStep(e) => e.address.parse().ok(),
    };

    let multicaller_address = multicaller_address.ok_or_eyre("MULTICALLER_ADDRESS_NOT_SET")?;
    //let private_key_encrypted = hex::decode(env::var("DATA")?)?;

    info!(address=?multicaller_address, "Multicaller");

    // Get flashbots relays from config
    let relays = topology_config
        .actors
        .broadcaster
        .as_ref()
        .and_then(|b| b.get("flashbots"))
        .map(|b| match b {
            BroadcasterConfig::Flashbots(f) => f.relays(),
        })
        .unwrap_or_default();

    let mut bc_actors = BlockchainActors::new(provider.clone(), bc.clone(), relays);
    bc_actors
        .mempool()?
        //.initialize_signers_with_encrypted_key(private_key_encrypted)? // initialize signer with encrypted key
        .with_block_history()? // collect blocks
        .with_price_station()? // calculate price fo tokens
        .with_health_monitor_pools()? // monitor pools health to disable empty
        .with_health_monitor_state()? // monitor state health
        .with_health_monitor_stuffing_tx()? // collect stuffing tx information
        .with_swap_encoder(Some(multicaller_address))? // convert swaps to opcodes and passes to estimator
        .with_evm_estimator()? // estimate gas, add tips
        .with_signers()? // start signer actor that signs transactions before broadcasting
        .with_flashbots_broadcaster(true)? // broadcast signed txes to flashbots
        .with_market_state_preloader()? // preload contracts to market state
        .with_nonce_and_balance_monitor()? // start monitoring balances of
        .with_pool_history_loader()? // load pools used in latest 10000 blocks
        .with_pool_protocol_loader()? // load curve + steth + wsteth
        .with_new_pool_loader()? // load new pools
        .with_swap_path_merger()? // load merger for multiple swap paths
        .with_diff_path_merger()? // load merger for different swap paths
        .with_same_path_merger()? // load merger for same swap paths with different stuffing txes
        .with_backrun_block()? // load backrun searcher for incoming block
        .with_backrun_mempool()? // load backrun searcher for mempool txes
    ;
    if let Some(influxdb_config) = topology_config.influxdb {
        bc_actors
            .with_influxdb_writer(influxdb_config.url, influxdb_config.database, influxdb_config.tags)?
            .with_block_latency_recorder()?;
    }

    bc_actors.wait().await;

    Ok(())
}
