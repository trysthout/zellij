use std::path::PathBuf;

use zellij_client::os_input_output::ClientOsApi;
use zellij_client::ClientInfo;
use zellij_utils::data::Style;
use zellij_utils::input::config::Config;
use zellij_utils::input::layout::Layout;
use zellij_utils::input::options::Options;
use zellij_utils::envs;
use zellij_utils::ipc::ClientAttributes;
use zellij_utils::ipc::ClientToServerMsg;
use log::info;

use crate::CliArgs;

pub fn client(
    mut os_input: Box<dyn ClientOsApi>,
    opts: zellij_utils::cli::CliArgs,
    config: Config,
    config_options: Options,
    layout: Option<Layout>,
    tab_position_to_focus: Option<usize>,
    pane_id_to_focus: Option<(u32, bool)>, // (pane_id, is_plugin)
    ipc: PathBuf,
)  {
    info!("Initialize Zellij client!");

    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let take_snapshot = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    os_input.unset_raw_mode(0).unwrap();

    //if !is_a_reconnect {
    //    // we don't do this for a reconnect because our controlling terminal already has the
    //    // attributes we want from it, and some terminals don't treat these atomically (looking at
    //    // your Windows Terminal...)
    //    let _ = os_input
    //        .get_stdout_writer()
    //        .write(take_snapshot.as_bytes())
    //        .unwrap();
    //    
    //    let _ = os_input
    //        .get_stdout_writer()
    //        .write(clear_client_terminal_attributes.as_bytes())
    //        .unwrap();
    //}

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


    //let first_msg = match info {
    //    //ClientInfo::Attach(name, config_options) => {
    //    //    envs::set_session_name(name.clone());
    //    //    os_input.update_session_name(name);
    //    //    //let ipc_pipe = create_ipc_pipe();

    //    //    ClientToServerMsg::AttachClient(
    //    //        client_attributes,
    //    //        config_options,
    //    //        tab_position_to_focus,
    //    //        pane_id_to_focus,
    //    //    )
    //    //        //ipc_pipe,
    //    //},
    //    
    //    ClientInfo::New(name) => {
    //        envs::set_session_name(name.clone());
    //        os_input.update_session_name(name);
    //        //let ipc_pipe = create_ipc_pipe();

    //        //spawn_server(&*ipc_pipe, opts.debug).unwrap();

    //        ClientToServerMsg::NewClient(
    //            client_attributes,
    //            Box::new(opts),
    //            Box::new(config_options.clone()),
    //            Box::new(layout.unwrap()),
    //                Some(config.plugins.clone()),
    //        )
    //    },
    //    _ => todo!()
    //};
    let name = opts.session.clone();
    envs::set_session_name(name.clone().unwrap());
    os_input.update_session_name(name.clone().unwrap());
    //let ipc_pipe = create_ipc_pipe();

    //spawn_server(&*ipc_pipe, opts.debug).unwrap();

    let first_msg = ClientToServerMsg::NewClient(
        client_attributes,
        Box::new(opts),
        Box::new(config_options.clone()),
        Box::new(layout.unwrap()),
            Some(config.plugins.clone()),
    );


    os_input.connect_to_server(&ipc);
    os_input.send_to_server(first_msg);
}