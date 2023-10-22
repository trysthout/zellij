
mod init;
use zellij::commands::{start_server, generate_unique_session_name, get_os_input};
use zellij_utils::clap::{self, Parser};
use zellij_utils::consts::*;
use zellij_utils::envs;
use zellij_utils::input::config::ConfigError;
use zellij_utils::logging::configure_logger;
use serde::Serialize;
use zellij_utils::setup::Setup;
use zellij_utils::shared::set_permissions;
use std::path::PathBuf;
use std::thread;
use zellij_utils::cli::validate_session;
use serde::Deserialize;
use zellij_client::{
    old_config_converter::{
        config_yaml_to_config_kdl, convert_old_yaml_files, layout_yaml_to_layout_kdl,
    },
    os_input_output::get_client_os_input,
    start_client as start_client_impl, ClientInfo,
};

use zellij_utils:: miette::{Report, Result};

#[derive(Parser, Default, Debug, Clone, Serialize, Deserialize)]
#[clap(version, name = "zellij-server")]
pub struct CliArgs {
    /// Specify name of a new session
    #[clap(long, short, overrides_with = "session", value_parser = validate_session)]
    pub session: Option<String>,


    /// Specify emitting additional debug information
    #[clap(short, long, value_parser)]
    pub debug: bool,

    /// Maximum panes on screen, caution: opening more panes will close old ones
    #[clap(long, value_parser)]
    pub max_panes: Option<usize>,
    
    /// Change where zellij looks for plugins
    #[clap(long, value_parser, overrides_with = "data_dir")]
    pub data_dir: Option<PathBuf>,
    
    /// Run server listening at the specified socket path
    #[clap(long, value_parser, hide = true, overrides_with = "server")]
    pub server: Option<PathBuf>,
    
    /// Name of a predefined layout inside the layout directory or the path to a layout file
    #[clap(short, long, value_parser, overrides_with = "layout")]
    pub layout: Option<PathBuf>,
    
    /// Change where zellij looks for the configuration file
    #[clap(short, long, overrides_with = "config", env = ZELLIJ_CONFIG_FILE_ENV, value_parser)]
    pub config: Option<PathBuf>,
    
    /// Change where zellij looks for the configuration directory
    #[clap(long, overrides_with = "config_dir", env = ZELLIJ_CONFIG_DIR_ENV, value_parser)]
    pub config_dir: Option<PathBuf>,
}

fn create_ipc_pipe() -> PathBuf {
    let mut sock_dir = ZELLIJ_SOCK_DIR.clone();
    std::fs::create_dir_all(&sock_dir).unwrap();
    set_permissions(&sock_dir, 0o700).unwrap();
    sock_dir.push(envs::get_session_name().unwrap());
    sock_dir
}

fn main() {
    //std::thread::sleep(std::time::Duration::from_millis(1000));
    //println!("122");
    configure_logger();
    let opts = CliArgs::parse(); 

    if let Some(name) = opts.session.clone() {
        envs::set_session_name(name);
    } else {
        envs::set_session_name(generate_unique_session_name())
    }

    let ipc = create_ipc_pipe();
    let ipc_copy1 = ipc.clone();
    let thread_join_handle = thread::spawn(move || {
        start_server(ipc, opts.debug)
    });

    let os_input = get_os_input(get_client_os_input);

    let zellij_cli_args = zellij_utils::cli::CliArgs {
        max_panes: opts.max_panes,
        data_dir: opts.data_dir,
        server: None,
        session: opts.session,
        layout: opts.layout,
        config: opts.config,
        config_dir: opts.config_dir,
        command: None,
        debug: opts.debug,
    };

    let (config, layout, config_options) = match Setup::from_cli_args(&zellij_cli_args) {
        Ok(results) => results,
        Err(e) => {
            if let ConfigError::KdlError(error) = e {
                let report: Report = error.into();
                eprintln!("{:?}", report);
            } else {
                eprintln!("{}", e);
            }
            std::process::exit(1);
        },
    };

    init::client(Box::new(os_input), zellij_cli_args, config, config_options, Some(layout), None,None, ipc_copy1);
    
    let _ =  thread_join_handle.join();
    
}

