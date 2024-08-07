use alloy_network::Network;
use alloy_primitives::BlockHash;
use alloy_provider::Provider;
use alloy_rpc_types::BlockId;
use alloy_transport::Transport;
use log::error;

use debug_provider::DebugProviderExt;
use defi_events::BlockStateUpdate;
use defi_types::debug_trace_block;
use loom_actors::{subscribe, Broadcaster, WorkerResult};

pub async fn new_node_block_state_worker<P, T, N>(
    client: P,
    block_hash_receiver: Broadcaster<BlockHash>,
    sender: Broadcaster<BlockStateUpdate>,
) -> WorkerResult
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N> + DebugProviderExt<T, N> + Send + Sync + Clone + 'static,
{
    subscribe!(block_hash_receiver);

    loop {
        if let Ok(block_hash) = block_hash_receiver.recv().await {
            let trace_result = debug_trace_block(client.clone(), BlockId::Hash(block_hash.into()), true).await;
            match trace_result {
                Ok((_, post)) => {
                    if let Err(e) = sender.send(BlockStateUpdate { block_hash, state_update: post }).await {
                        error!("Broadcaster error {}", e)
                    }
                }
                Err(e) => {
                    error!("debug_trace_block error : {e}")
                }
            }
        }
    }
}
