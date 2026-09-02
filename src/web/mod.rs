mod api;
mod assets;
pub mod model;
pub mod repo;
mod server;

pub use server::{open_browser, run_web_server, run_web_server_and_open};
