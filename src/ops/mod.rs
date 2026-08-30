pub mod lifecycle;
pub mod status;

pub use lifecycle::{
    auto_deploy_to_bin, disable_service, install_service, restart_service, uninstall_service,
};
pub use status::print_status;
