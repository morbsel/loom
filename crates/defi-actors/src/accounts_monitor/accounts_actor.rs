use std::marker::PhantomData;
use std::time::Duration;

use alloy_eips::{BlockId, BlockNumberOrTag};
use alloy_network::Network;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_rpc_types::BlockTransactions;
use alloy_transport::Transport;
use async_trait::async_trait;
use log::debug;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::sleep;

use defi_blockchain::Blockchain;
use defi_entities::{AccountNonceAndBalanceState, BlockHistory};
use defi_events::MarketEvents;
use loom_actors::{Accessor, Actor, ActorResult, Broadcaster, Consumer, SharedState, WorkerResult};
use loom_actors_macros::{Accessor, Consumer};

pub async fn nonce_and_balance_fetcher_worker<P, T, N>(client: P, accounts_state: SharedState<AccountNonceAndBalanceState>) -> WorkerResult
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N> + Send + Sync + Clone + 'static,
{
    let eth_addr = Address::ZERO;

    loop {
        let accounts = accounts_state.read().await.get_accounts_vec();
        for addr in accounts.into_iter() {
            let nonce = client.get_transaction_count(addr).block_id(BlockId::Number(BlockNumberOrTag::Latest)).await;
            let balance = client.get_balance(addr).block_id(BlockId::Number(BlockNumberOrTag::Latest)).await;

            if let Some(acc) = accounts_state.write().await.get_mut_account(&addr) {
                if let Ok(nonce) = nonce {
                    acc.set_nonce(nonce);
                }
                if let Ok(balance) = balance {
                    acc.set_balance(eth_addr, balance);
                }
            };
            debug!("Account {} nonce {:?} balance {:?}", addr, nonce, balance);
        }

        sleep(Duration::from_secs(20)).await;
    }
}

pub async fn nonce_and_balance_monitor_worker(
    accounts_state: SharedState<AccountNonceAndBalanceState>,
    block_history_state: SharedState<BlockHistory>,
    market_events_rx: Broadcaster<MarketEvents>,
) -> WorkerResult {
    let mut market_events = market_events_rx.subscribe().await;

    loop {
        tokio::select! {
            msg = market_events.recv() => {
                let market_event_msg : Result<MarketEvents, RecvError> = msg;
                   if let Ok(MarketEvents::BlockTxUpdate{ block_hash, .. }) =  market_event_msg {
                        if let Some(block_entry) = block_history_state.read().await.get_market_history_entry(&block_hash).cloned() {
                            if let Some(block) = block_entry.block {
                                if let BlockTransactions::Full(txs) = block.transactions {
                                    for tx in txs {
                                        let tx_from : Address = tx.from;
                                        if accounts_state.read().await.is_monitored(&tx_from) {
                                            if let Some(&mut ref mut account) = accounts_state.write().await.get_mut_account(&tx_from) {
                                                let spent = (tx.max_fee_per_gas.unwrap() + tx.max_priority_fee_per_gas.unwrap()) * tx.gas + tx.value.to::<u128>();
                                                account.sub_balance(Address::ZERO, U256::from(spent));
                                                account.set_nonce(tx.nonce);
                                            }
                                        }

                                        if let Some(to )  = tx.to {
                                            if accounts_state.read().await.is_monitored(&to) {
                                                if let Some(&mut ref mut account) = accounts_state.write().await.get_mut_account(&to) {
                                                    account.add_balance(Address::ZERO, tx.value);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                    }
                }
            }
        }
    }
}

#[derive(Accessor, Consumer)]
pub struct NonceAndBalanceMonitorActor<P, T, N> {
    client: P,
    #[accessor]
    accounts_nonce_and_balance: Option<SharedState<AccountNonceAndBalanceState>>,
    #[accessor]
    block_history: Option<SharedState<BlockHistory>>,
    #[consumer]
    market_events: Option<Broadcaster<MarketEvents>>,
    _t: PhantomData<T>,
    _n: PhantomData<N>,
}

impl<P, T, N> NonceAndBalanceMonitorActor<P, T, N>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N> + Send + Sync + Clone + 'static,
{
    pub fn new(client: P) -> NonceAndBalanceMonitorActor<P, T, N> {
        NonceAndBalanceMonitorActor {
            client,
            accounts_nonce_and_balance: None,
            block_history: None,
            market_events: None,
            _t: PhantomData,
            _n: PhantomData,
        }
    }

    pub fn on_bc(self, bc: &Blockchain) -> NonceAndBalanceMonitorActor<P, T, N> {
        NonceAndBalanceMonitorActor {
            accounts_nonce_and_balance: Some(bc.nonce_and_balance()),
            block_history: Some(bc.block_history().clone()),
            market_events: Some(bc.market_events_channel().clone()),
            ..self
        }
    }
}

#[async_trait]
impl<P, T, N> Actor for NonceAndBalanceMonitorActor<P, T, N>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N> + Send + Sync + Clone + 'static,
{
    fn start(&self) -> ActorResult {
        let monitor_task = tokio::task::spawn(nonce_and_balance_monitor_worker(
            self.accounts_nonce_and_balance.clone().unwrap(),
            self.block_history.clone().unwrap(),
            self.market_events.clone().unwrap(),
        ));

        let fetcher_task =
            tokio::task::spawn(nonce_and_balance_fetcher_worker(self.client.clone(), self.accounts_nonce_and_balance.clone().unwrap()));

        Ok(vec![monitor_task, fetcher_task])
    }

    fn name(&self) -> &'static str {
        "NonceAndBalanceMonitorActor"
    }
}
