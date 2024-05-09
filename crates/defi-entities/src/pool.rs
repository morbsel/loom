use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

use alloy_primitives::{Address, Bytes, U256};
use eyre::{ErrReport, eyre, Result};
use revm::InMemoryDB;
use revm::primitives::Env;

use crate::required_state::RequiredState;

#[derive(Clone)]
pub struct EmptyPool {
    address: Address,
}

impl EmptyPool {
    pub fn new(address: Address) -> Self {
        EmptyPool {
            address
        }
    }
}


impl Pool for EmptyPool {
    fn get_address(&self) -> Address {
        self.address
    }

    fn calculate_out_amount(&self, state: &InMemoryDB, env: Env, token_address_from: &Address, token_address_to: &Address, in_amount: U256) -> eyre::Result<(U256, u64), ErrReport> {
        Err(eyre!("NOT_IMPLEMENTED"))
    }

    fn calculate_in_amount(&self, state: &InMemoryDB, env: Env, token_address_from: &Address, token_address_to: &Address, out_amount: U256) -> eyre::Result<(U256, u64), ErrReport> {
        Err(eyre!("NOT_IMPLEMENTED"))
    }

    fn can_flash_swap(&self) -> bool {
        false
    }

    fn get_encoder(&self) -> &dyn AbiSwapEncoder {
        &DefaultAbiSwapEncoder {}
    }

    fn get_state_required(&self) -> Result<RequiredState> {
        Ok(RequiredState::new())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum PoolClass {
    Unknown,
    UniswapV2,
    UniswapV3,
    Curve,
    LidoStEth,
    LidoWstEth,
    RocketPool,
    //NomiswapStable,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PoolProtocol {
    Unknown,
    UniswapV2,
    UniswapV2Like,
    NomiswapStable,
    Sushiswap,
    SushiswapV3,
    DooarSwap,
    Safeswap,
    Miniswap,
    Shibaswap,
    UniswapV3,
    UniswapV3Like,
    PancakeV3,
    Integral,
    Maverick,
    Curve,
    LidoStEth,
    LidoWstEth,
    RocketEth,
}

impl Display for PoolProtocol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let protocol_name = match self {
            Self::Unknown => "Unknown",
            Self::UniswapV2 => "UniswapV2",
            Self::UniswapV2Like => "UniswapV2Like",
            Self::UniswapV3 => "UniswapV3",
            Self::PancakeV3 => "PancakeV3",
            Self::UniswapV3Like => "UniswapV3Like",
            Self::NomiswapStable => "NomiswapStable",
            Self::Sushiswap => "Sushiswap",
            Self::SushiswapV3 => "SushiswapV3",
            Self::DooarSwap => "Dooarswap",
            Self::Miniswap => "Miniswap",
            Self::Shibaswap => "Shibaswap",
            Self::Safeswap => "Safeswap",
            Self::Integral => "Integral",
            Self::Maverick => "Maverick",
            Self::Curve => "Curve",
            Self::LidoWstEth => "WstEth",
            Self::LidoStEth => "StEth",
            Self::RocketEth => "RocketEth",
            _ => { "Unknown" }
        };
        write!(f, "{}", protocol_name)
    }
}

pub struct PoolWrapper {
    pub pool: Arc<dyn Pool>,
}

impl PartialOrd for PoolWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for PoolWrapper {}

impl Ord for PoolWrapper {
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_address().cmp(&other.get_address())
    }
}

impl Display for PoolWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{:?}", self.get_protocol(), self.get_address())
    }
}

impl Debug for PoolWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{:?}", self.get_protocol(), self.get_address())
    }
}

impl Hash for PoolWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_address().hash(state)
    }
}

impl PartialEq for PoolWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.pool.get_address() == other.pool.get_address()
    }
}

impl PoolWrapper {
    pub fn new(pool: Arc<dyn Pool>) -> Self {
        PoolWrapper {
            pool
        }
    }

    pub fn empty(pool_address: Address) -> Self {
        let pool = EmptyPool::new(pool_address);
        Self::new(Arc::new(pool))
    }
}


impl Clone for PoolWrapper {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

impl Deref for PoolWrapper {
    type Target = dyn Pool;

    fn deref(&self) -> &Self::Target {
        self.pool.deref()
    }
}

impl<T: 'static + Pool + Clone> From<T> for PoolWrapper {
    fn from(pool: T) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }
}


pub trait Pool: Sync + Send
{
    fn get_class(&self) -> PoolClass {
        PoolClass::Unknown
    }

    fn get_protocol(&self) -> PoolProtocol {
        PoolProtocol::Unknown
    }

    //fn clone_box(&self) -> Box<dyn Pool>;

    fn get_address(&self) -> Address;

    fn get_fee(&self) -> U256 {
        U256::ZERO
    }

    fn get_tokens(&self) -> Vec<Address> {
        Vec::new()
    }

    fn get_swap_directions(&self) -> Vec<(Address, Address)> {
        return Vec::new();
    }

    fn calculate_out_amount(&self, state: &InMemoryDB, env: Env, token_address_from: &Address, token_address_to: &Address, in_amount: U256) -> Result<(U256, u64), ErrReport>;

    // returns (in_amount, gas_used)
    fn calculate_in_amount(&self, state: &InMemoryDB, env: Env, token_address_from: &Address, token_address_to: &Address, out_amount: U256) -> Result<(U256, u64), ErrReport>;

    fn can_flash_swap(&self) -> bool;

    fn can_calculate_in_amount(&self) -> bool {
        return true;
    }


    fn get_encoder(&self) -> &dyn AbiSwapEncoder;

    fn get_read_only_cell_vec(&self) -> Vec<U256> {
        Vec::new()
    }

    fn get_state_required(&self) -> Result<RequiredState>;
}
/*
#[async_trait]
pub trait PoolSetup<P: Provider + Send + Sync + Clone + 'static>: Pool {
    async fn fetch_state_required(&self, client: P, block_number: Option<BlockNumber>) -> Result<GethStateUpdate>;
    /*async fn fetch_pool_data(client: P, address: Address) -> Result<PoolWrapper> {
        Err(ErrReport::msg("HE"))
    }*/
    /*async fn fetch_pool_data_evm<D>(&mut self, db : &CacheDB<D>, env : Env) -> Result<()>
    where
        D : DatabaseRef<Error=Infallible> + Clone + Default + Send + Sync + 'static,
        <D as DatabaseRef>::Error : Debug
    {
        Err(ErrReport::msg("HE"))
    }
     */
}
*/
pub struct DefaultAbiSwapEncoder {}

impl AbiSwapEncoder for DefaultAbiSwapEncoder {}

#[derive(Clone, Debug, PartialEq)]
pub enum PreswapRequirement {
    Unknown,
    Transfer(Address),
    Allowance,
    Callback,
    Base,
}

pub trait AbiSwapEncoder {
    fn encode_swap_in_amount_provided(&self, token_from_address: Address, token_to_address: Address, amount: U256, recipient: Address, payload: Bytes) -> Result<Bytes> {
        Err(eyre!("NOT_IMPLEMENTED"))
    }
    fn encode_swap_out_amount_provided(&self, token_from_address: Address, token_to_address: Address, amount: U256, recipient: Address, payload: Bytes) -> Result<Bytes> {
        Err(eyre!("NOT_IMPLEMENTED"))
    }
    fn preswap_requirement(&self) -> PreswapRequirement {
        PreswapRequirement::Unknown
    }

    fn is_native(&self) -> bool {
        false
    }

    fn swap_in_amount_offset(&self, token_from_address: Address, token_to_address: Address) -> Option<u32> {
        None
    }
    fn swap_out_amount_offset(&self, token_from_address: Address, token_to_address: Address) -> Option<u32> {
        None
    }
    fn swap_out_amount_return_offset(&self, token_from_address: Address, token_to_address: Address) -> Option<u32> {
        None
    }
    fn swap_in_amount_return_offset(&self, token_from_address: Address, token_to_address: Address) -> Option<u32> {
        None
    }
    fn swap_out_amount_return_script(&self, token_from_address: Address, token_to_address: Address) -> Option<Bytes> {
        None
    }
    fn swap_in_amount_return_script(&self, token_from_address: Address, token_to_address: Address) -> Option<Bytes> {
        None
    }
}