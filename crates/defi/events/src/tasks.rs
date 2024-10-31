use alloy_primitives::Address;
use loom_defi_entities::PoolClass;

#[derive(Clone, Debug)]
pub enum Task {
    FetchAndAddPools(Vec<(Address, PoolClass)>),
    FetchStateAndAddPools(Vec<(Address, PoolClass)>),
}