//! 隧道服务：事件定义与运行时操作。

mod events;
mod runtime;

pub use events::AppEvent;
pub use runtime::{spawn_close, spawn_event_forwarder, spawn_host, spawn_join};
