mod init;
use init::init_server;

use zellij_server_command::CliArgs;
use zellij_utils::clap::Parser;
use zellij_utils::logging::configure_logger;



fn main() {
    configure_logger();
    let opts = CliArgs::parse(); 

    let join_handle = init_server(opts);
    
    join_handle.join().unwrap()
}

