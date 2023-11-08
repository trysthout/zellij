use std::path::PathBuf;
use std::thread::JoinHandle;

use log::info;
use std::thread;
use zellij::commands::{generate_unique_session_name, get_os_input, start_server};
use zellij_client::os_input_output::get_client_os_input;
use zellij_client::os_input_output::ClientOsApi;
use zellij_utils::consts::*;
use zellij_utils::data::Style;
use zellij_utils::envs;
use zellij_utils::input::config::Config;
use zellij_utils::input::config::ConfigError;
use zellij_utils::input::layout::Layout;
use zellij_utils::input::options::Options;
use zellij_utils::ipc::ClientAttributes;
use zellij_utils::ipc::ClientToServerMsg;
use zellij_utils::setup::Setup;
use zellij_utils::shared::set_permissions;

use crate::CliArgs;

use zellij_utils::miette::Report;

fn create_ipc_pipe() -> PathBuf {
    let mut sock_dir = ZELLIJ_SOCK_DIR.clone();
    std::fs::create_dir_all(&sock_dir).unwrap();
    set_permissions(&sock_dir, 0o700).unwrap();
    sock_dir.push(envs::get_session_name().unwrap());
    sock_dir
}

pub fn init_server(opts: CliArgs) -> JoinHandle<()> {
    if let Some(name) = opts.session.clone() {
        envs::set_session_name(name);
    } else {
        envs::set_session_name(generate_unique_session_name())
    }

    let thread_join_handle = thread::spawn(move || start_server(create_ipc_pipe(), opts.debug));

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
                eprintln!("{report:?}");
            } else {
                eprintln!("{e}");
            }
            std::process::exit(1);
        },
    };

    init_client(
        Box::new(os_input),
        zellij_cli_args,
        config,
        config_options,
        Some(layout),
        None,
        None,
        create_ipc_pipe(),
    );
    thread_join_handle
}

pub fn init_client(
    os_input: Box<dyn ClientOsApi>,
    opts: zellij_utils::cli::CliArgs,
    config: Config,
    config_options: Options,
    layout: Option<Layout>,
    _tab_position_to_focus: Option<usize>,
    _pane_id_to_focus: Option<(u32, bool)>, // (pane_id, is_plugin)
    ipc: PathBuf,
) {
    info!("Initialize Zellij client!");

    envs::set_zellij("0".to_string());
    config.env.set_vars();

    let palette = config
        .theme_config(&config_options)
        .unwrap_or_else(|| os_input.load_palette());

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    let client_attributes = ClientAttributes {
        size: full_screen_ws,
        style: Style {
            colors: palette,
            rounded_corners: config.ui.pane_frames.rounded_corners,
            hide_session_name: config.ui.pane_frames.hide_session_name,
        },
        keybinds: config.keybinds.clone(),
    };

    //let name = opts.session.clone();
    //envs::set_session_name(name.clone().unwrap());
    //os_input.update_session_name(name.clone().unwrap());
    //let ipc_pipe = create_ipc_pipe();

    //spawn_server(&*ipc_pipe, opts.debug).unwrap();

    let first_msg = ClientToServerMsg::NewClient(
        client_attributes,
        Box::new(opts),
        Box::new(config_options),
        Box::new(layout.unwrap()),
        Some(config.plugins),
    );

    os_input.connect_to_server(&ipc);
    os_input.send_to_server(first_msg);
    os_input.send_to_server(ClientToServerMsg::DetachSession(vec![1]))
}
