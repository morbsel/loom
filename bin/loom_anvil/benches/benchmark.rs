use std::collections::HashMap;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, BlockNumber, U256};
use chrono::Local;
use criterion::{BenchmarkId, black_box, Criterion, criterion_group, criterion_main};
use criterion::async_executor::AsyncExecutor;
use eyre::Result;
use log::{debug, error, info};
use rand::prelude::{Rng, SeedableRng, StdRng};
use rand::thread_rng;
use rayon::{ThreadPool, ThreadPoolBuilder};
use rayon::prelude::*;
use revm::db::{CacheDB, EmptyDB};
use revm::primitives::Env;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use debug_provider::AnvilControl;
use defi_entities::{MarketState, Pool, PoolWrapper};
use defi_entities::required_state::RequiredStateReader;
use defi_pools::{UniswapV2Pool, UniswapV3Pool};
use defi_pools::protocols::UniswapV3Protocol;

async fn performance_test() {
    let mut rng = StdRng::from_entropy();

    // Initializing 60,000 structures with random U256 and unique generated ethers::Address
    let mut map = HashMap::new();
    for _ in 0..60_000 {
        //let random_u256 = rng.gen::<U256>();
        //let address = rng.gen::<Address>();


        let random_bytes: [u8; 32] = rng.gen();
        let random_u256 = U256::from_be_bytes(random_bytes);
        let random_address_bytes: [u8; 20] = rng.gen();
        let address = Address::from(&random_address_bytes);

        map.insert(address, random_u256);
    }

    // Collect all values from the HashMap
    let mut values: Vec<U256> = map.values().cloned().collect();

    // Sort values by random U256
    values.sort_unstable();
}


async fn fetch_data_and_pool() -> Result<(MarketState, PoolWrapper)> {
    //let provider = Provider::<Ws>::connect_with_reconnects("ws://honey3.loop:8008/looper", 10).await.unwrap();

    let block_number: BlockNumber = 19948348;

    let client = AnvilControl::from_node_on_block("ws://falcon.loop:8008/looper".to_string(), block_number).await?;

    let mut market_state = MarketState::new(CacheDB::new(EmptyDB::default()));

    market_state.add_state(&UniswapV3Protocol::get_quoter_v3_state());


    let pool_address: Address = "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640".parse().unwrap();
    //let pool_address: Address = "0x5777d92f208679db4b9778590fa3cab3ac9e2168".parse().unwrap();
    let mut pool = UniswapV3Pool::fetch_pool_data(client.clone(), pool_address).await?;

    //let pool_address: Address = "0x9c2dc3d5ffcecf61312c5f4c00660695b32fb3d1".parse().unwrap();
    //let pool_address: Address = "0xa478c2975ab1ea89e8196811f51a7b7ade33eb11".parse().unwrap();
    //let mut pool = UniswapV2Pool::fetch_pool_data(client.clone(), pool_address).await?;


    let state_required = pool.get_state_required()?;

    let state_required = RequiredStateReader::fetch_calls_and_slots(client.clone(), state_required, Some(block_number)).await?;

    market_state.add_state(&state_required);

    Ok((market_state, PoolWrapper::new(Arc::new(pool))))
}

async fn sync_run(state_db: &CacheDB<EmptyDB>, pool: UniswapV3Pool) {
    let evm_env = Env::default();
    let mut step = U256::from(U256::from(10).pow(U256::from(18)));
    let mut in_amount = U256::from(U256::from(10).pow(U256::from(18)));
    for i in 0..10 {
        pool.calculate_out_amount(state_db, evm_env.clone(), &pool.token1, &pool.token0, in_amount).unwrap();
        /*match pool.calculate_out_amount(state_db, evm_env.clone(), &pool.token1, &pool.token0, in_amount) {
            Ok(v)=>{println!("{v}")}
            Err(e)=>{println!("{e}")}
        }*/
        in_amount += step;
    }
    //println!("{}", out_amount);
}

async fn rayon_run(state_db: &CacheDB<EmptyDB>, pool: PoolWrapper, threadpool: Arc<ThreadPool>) {
    let start_time = chrono::Local::now();
    let evm_env = Env::default();
    let mut step = U256::from(U256::from(10).pow(U256::from(16)));
    let mut in_amount = U256::from(U256::from(10).pow(U256::from(18)));
    //let mut step = U256::from(U256::from(10).pow(U256::from(7)));
    //let mut in_amount = U256::from(U256::from(10).pow(U256::from(8)));

    const ITER_COUNT: usize = 10000;

    let range = 0..ITER_COUNT;

    let in_vec: Vec<U256> = range.map(|i| in_amount + (step * U256::from(i))).collect();


    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel::<U256>(ITER_COUNT / 1);

    let state_db_clone = state_db.clone();

    let tokens = pool.get_tokens();
    let token_from = tokens[1];
    let token_to = tokens[0];


    tokio::task::spawn(async move {
        threadpool.install(|| {
            in_vec.into_par_iter().for_each_with((&state_db_clone, &evm_env, &result_tx), |req, in_amount| {
                //let mut rng = thread_rng();
                //let random_number: u32 = rng.gen();
                //let in_amount = in_amount + U256::from(random_number);

                let out_amount = pool.calculate_out_amount(req.0, req.1.clone(), &token_from, &token_to, in_amount).unwrap();
                match req.2.try_send(out_amount.0) {
                    Err(e) => { error!("{e}") }
                    _ => {}
                }
            });
        });
        drop(result_tx);
    });

    //drop(result_tx);

    let mut counter: usize = 0;


    while let Some(result) = result_rx.recv().await {
        counter += 1;
    }
    let time_spent = chrono::Local::now() - start_time;
    let calc_per_sec = time_spent / (counter as i32);
    println!("Iterations : {counter} Took: {time_spent} Per sec: {calc_per_sec}");
    assert_eq!(counter, ITER_COUNT, "NOT_ALL_RESULTS");
}


async fn rayon_parallel_run<'a>(state_db: &CacheDB<EmptyDB>, pool: PoolWrapper) {
    const TASKS_COUNT: u32 = 3;
    let mut tasks: Vec<JoinHandle<_>> = Vec::new();

    let cpus = num_cpus::get();
    println!("Cpus {cpus}");
    let threadpool = Arc::new(ThreadPoolBuilder::new().num_threads(cpus - 2).build().unwrap());


    for i in 0..TASKS_COUNT {
        let pool_clone = pool.clone();
        let state_db_clone = state_db.clone();
        let threadpool_ptr = threadpool.clone();
        tasks.push(
            tokio::task::spawn(async move {
                let start_time = Local::now();
                println!("Task {i} started {start_time}");
                rayon_run(&state_db_clone, pool_clone, threadpool_ptr).await;
                let finish_time = Local::now();
                println!("Task {i} finished {finish_time} elapsed : {}", finish_time - start_time);
            })
        );
    }
    for t in tasks {
        match t.await {
            Ok(_) => {}
            Err(_) => {
                panic!("TASK_FAILED")
            }
        }
    }
}


async fn tokio_run(state_db: &CacheDB<EmptyDB>, pool: UniswapV3Pool) {
    let evm_env = Env::default();
    let mut step = U256::from(U256::from(10).pow(U256::from(16)));
    let mut in_amount = U256::from(U256::from(10).pow(U256::from(18)));

    const ITER_COUNT: usize = 10000;
    const WORKERS_COUNT: usize = 10;

    let (request_tx, mut request_rx) = tokio::sync::mpsc::channel::<Option<(Arc<CacheDB<EmptyDB>>, Arc<Env>, U256)>>(ITER_COUNT);
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel::<U256>(ITER_COUNT);

    let request_rx = Arc::new(RwLock::new(request_rx));
    let result_tx = Arc::new(result_tx);

    for i in 0..WORKERS_COUNT {
        let mut request_rx_clone = request_rx.clone();
        let result_tx_ptr = result_tx.clone();
        let pool = pool.clone();
        tokio::task::spawn(async move {
            loop {
                let mut request_rx_guard = request_rx_clone.write().await;
                match request_rx_guard.recv().await {
                    Some(req) => {
                        //println!("Recv {i}");
                        drop(request_rx_guard);
                        match req {
                            Some(req) => {
                                let out_amount = pool.calculate_out_amount(req.0.deref(), req.1.as_ref().clone(), &pool.token1, &pool.token0, req.2).unwrap();
                                match result_tx_ptr.try_send(out_amount.0) {
                                    Err(e) => { println!("result_tx_ptr error: {e}") }
                                    _ => {}
                                }
                            }
                            None => {
                                drop(result_tx_ptr);
                                break;
                            }
                        }
                    }
                    None => { break; }
                }
            }
            //println!("Worker {i} finished");
        });
    }

    drop(result_tx);


    let range = 0..ITER_COUNT;
    let in_vec: Vec<U256> = range.map(|i| in_amount + (step * U256::from(i))).collect();

    let env_clone = Arc::new(evm_env);
    let state_db_clone = Arc::new(state_db.clone());


    for in_amount in in_vec.into_iter() {
        match request_tx.try_send(Some((state_db_clone.clone(), env_clone.clone(), in_amount))) {
            Err(e) => { println!("error : {e}") }
            _ => {}
        }
    }

    for w in 0..WORKERS_COUNT {
        match request_tx.send(None).await {
            Err(e) => { println!("error : {e}") }
            _ => {}
        }
    }
    println!("Broadcasting finished");

    let mut counter = 0;

    while let Some(result) = result_rx.recv().await {
        counter += 1;
        //println!("Result received {counter}");
    }
    println!("{counter}");
    assert_eq!(counter, ITER_COUNT, "NOT_ALL_RESULTS");
}


async fn tokio_parallel_run(state_db: &CacheDB<EmptyDB>, pool: UniswapV3Pool) {
    const TASKS_COUNT: u32 = 3;
    let mut tasks: Vec<JoinHandle<_>> = Vec::new();


    for i in 0..TASKS_COUNT {
        let pool_clone = pool.clone();
        let state_db_clone = state_db.clone();
        tasks.push(
            tokio::task::spawn(async move {
                let start_time = Local::now();
                println!("Tokio Task {i} started {start_time}");
                tokio_run(&state_db_clone, pool_clone).await;
                let finish_time = Local::now();
                println!("Tokio Task {i} finished {finish_time} elapsed : {}", finish_time - start_time);
            })
        );
    }
    for t in tasks {
        match t.await {
            Ok(_) => {}
            Err(_) => {
                panic!("TASK_FAILED")
            }
        }
    }
}

#[cfg(not(test))]
fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool speed");
    group.measurement_time(Duration::from_secs(60));


    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let fetch_result = rt.block_on(
        fetch_data_and_pool()
    ).unwrap();

    let cache_db = fetch_result.0.state_db;
    let pool = fetch_result.1;
    /*
    group.bench_with_input(BenchmarkId::from_parameter(fetch_result), i, |b, &i| {
        b.iter(|| flow( black_box(&cache_db), black_box(pool)))
    });

     */
    group.bench_function("tokio_parallel_run", |b|
        b.iter(|| rt.block_on(sync_run(black_box(&cache_db), black_box(pool.clone())))),
    );

    group.finish();
}

#[cfg(not(test))]
criterion_group!(benches, benchmark);
#[cfg(not(test))]
criterion_main!(benches);


#[cfg(test)]
#[tokio::main]
async fn main() {
    println!("Running tests, not benchmarks");
    let fetch_result = fetch_data_and_pool().await.unwrap();
    let cache_db = fetch_result.0.state_db;
    let pool = fetch_result.1;

    let start_time = chrono::Local::now();
    rayon_parallel_run(&cache_db, pool).await;
    println!("Execution time : {}", chrono::Local::now() - start_time);
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_run() {
        println!("Running test");
    }

    #[tokio::test]
    async fn test_flow() {
        println!("Running test_flow");
        let fetch_result = fetch_data_and_pool().await.unwrap();
        let cache_db = fetch_result.0.state_db;
        let pool = fetch_result.1;

        rayon_parallel_run(&cache_db, pool).await
    }
}