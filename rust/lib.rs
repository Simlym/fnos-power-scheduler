pub mod schedule;
pub mod store;
pub mod system;
pub mod wire;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
