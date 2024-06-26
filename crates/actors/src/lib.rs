pub use actor::{Accessor, Actor, ActorResult, Consumer, Producer, WorkerResult};
pub use channels::{Broadcaster, MultiProducer};
pub use shared_state::SharedState;

mod actor;
mod channels;
mod shared_state;

#[macro_export]
macro_rules! run_async {
    ($fx:expr) => {
        match $fx.await {
            Ok(_) => {},
            Err(e) => error!("{:?}", e),
        }
    };
}

