use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use russh::server::Handle;
use tokio::sync::mpsc::UnboundedReceiver;

use std::sync::Mutex;
use zellij_server_command::init::init_server;
use zellij_server_command::CliArgs;
use zellij_utils::cli::{SessionCommand, Sessions};
use zellij_utils::input::actions::Action;
use zellij_utils::input::config::{Config};
use zellij_utils::input::options::Options;
use zellij_utils::setup::Setup;

use crate::handler::HandlerEvent;
use crate::ssh::SshInputOutput;
use crate::zellij_session::{
    get_active_session, get_sessions_sorted_by_mtime, list_sessions, match_session_name,
    ActiveSession, SessionNameMatch,
};
use crate::{PtyRequest, ServerChannelId, ServerHandle};
use russh::{ChannelId, CryptoVec, Sig};
use tokio::sync::mpsc::*;

pub enum ZellijClientData {
    Data(String),
    Exit,
}

pub struct Session {
    handle: Option<Handle>,
    zellij_cli_args: CliArgs,
    pty_request: Option<PtyRequest>,
    channel_id: Option<ServerChannelId>,
    rx: UnboundedReceiver<HandlerEvent>,
    recv: UnboundedReceiver<(Option<String>, Option<()>)>,
    sender: UnboundedSender<(Option<String>, Option<()>)>,
    server_sender: crossbeam_channel::Sender<Vec<u8>>,
    server_receiver: crossbeam_channel::Receiver<Vec<u8>>,
    server_signal_sender: crossbeam_channel::Sender<Sig>,
    server_signal_receiver: crossbeam_channel::Receiver<Sig>,
}

impl Session {
    pub fn new(args: CliArgs, rx: UnboundedReceiver<HandlerEvent>) -> Self {
        let (sender, recv) = unbounded_channel();
        let (server_sender, server_receiver) = crossbeam_channel::unbounded::<Vec<u8>>();
        let (server_signal_sender, server_signal_receiver) = crossbeam_channel::unbounded::<Sig>();

        Self {
            zellij_cli_args: args,
            rx,
            handle: None,
            channel_id: None,
            sender,
            recv,
            server_receiver,
            server_sender,
            pty_request: None,
            server_signal_sender,
            server_signal_receiver,
        }
    }

    pub async fn run(&mut self) {
        loop {
            if let Some(event) = self.rx.recv().await {
                self.handle_handler_event(event, self.zellij_cli_args.clone())
                    .await
            }
        }
    }

    async fn handle_handler_event(&mut self, event: HandlerEvent, args: CliArgs) {
        match event {
            HandlerEvent::Authenticated(handle, tx) => {
                self.handle = Some(handle.0);

                if envs::get_session_name().is_err() {
                    self.start_zellij_server();
                }
                let _ = tx.send(());
            },
            HandlerEvent::PtyRequest(channel_id, pty_request) => {
                self.pty_request = Some(pty_request);
                self.channel_id = Some(channel_id);
            },
            HandlerEvent::ShellRequest(channel_id) => {
                let result = handle_openpty();
                let (sender, mut recv) = unbounded_channel::<(Option<String>, Option<()>)>();
                let pty_request = self.pty_request.as_ref().unwrap();
                let win_size = Winsize {
                    ws_row: pty_request.row_height as u16,
                    ws_col: pty_request.col_width as u16,
                    ws_xpixel: pty_request.pix_width as u16,
                    ws_ypixel: pty_request.pix_height as u16,
                };
                let handle = self.handle.clone().unwrap();
                let server_receiver = self.server_receiver.clone();
                let server_signal_receiver = self.server_signal_receiver.clone();
                std::thread::spawn(move || {
                    Self::start_zellij_client(
                        args,
                        result,
                        sender,
                        server_receiver,
                        server_signal_receiver,
                        ServerHandle(handle),
                        channel_id.0,
                        win_size,
                    );
                });

                let handle = self.handle.clone().unwrap();
                let channel_id = self.channel_id.unwrap().0;
                tokio::spawn(async move {
                    loop {
                        if let Some(event) = recv.recv().await {
                            if let Some(data) = event.0 {
                                let _ = handle.data(channel_id, CryptoVec::from(data)).await;
                                continue;
                            }

                            if event.1.is_some() {
                                //let _ = handle.data(channel_id, CryptoVec::from(event)).await;
                                let _ = handle.close(channel_id).await;
                            }
                        }
                    }
                });
            },
            HandlerEvent::Data(_channel_id, data) => {
                let _ = self.server_sender.send(data);
            },
            HandlerEvent::WindowChangeRequest(_, _winsize) => {},
            HandlerEvent::Signal(_, signal) => {
                let _ = self.server_signal_sender.send(signal);
            },
        }
    }

    fn start_zellij_server(&self) {
        init_server(self.zellij_cli_args.clone());
    }

    fn start_zellij_client(
        args: CliArgs,
        pty: OpenptyResult,
        sender: UnboundedSender<(Option<String>, Option<()>)>,
        server_receiver: crossbeam_channel::Receiver<Vec<u8>>,
        server_signal_receiver: crossbeam_channel::Receiver<Sig>,
        handle: ServerHandle,
        channel_id: ChannelId,
        win_size: Winsize,
    ) {
        use zellij_client::start_client_ssh;
        use zellij_utils::cli::Command;
        use zellij_utils::data::ConnectToSession;

        let zellij_cli_args = zellij_utils::cli::CliArgs {
            max_panes: args.max_panes,
            data_dir: args.data_dir.clone(),
            server: None,
            session: args.session.clone(),
            layout: args.layout.clone(),
            config: args.config.clone(),
            config_dir: args.config_dir.clone(),
            command: Some(Command::Sessions(Sessions::Attach {
                session_name: args.session.clone(),
                create: false,
                force_run_commands: false,
                index: None,
                options: None,
            })),
            debug: args.debug,
        };

        let (config, layout, config_options) = match Setup::from_cli_args(&zellij_cli_args) {
            Ok(results) => results,
            Err(_e) => {
                //if let ConfigError::KdlError(error) = e {
                //    let report: Report = error.into();
                //    eprintln!("{:?}", report);
                //} else {
                //    eprintln!("{}", e);
                //}
                process::exit(1);
            },
        };
        let reconnect_to_session: Option<ConnectToSession> = None;
        //let os_input = get_os_input(get_client_os_input);
        let os_input = get_server_input(
            handle,
            channel_id,
            win_size,
            sender.clone(),
            server_receiver,
            server_signal_receiver,
        );
        //let handle = self.handle.clone().unwrap();

        //loop {
        let os_input = os_input;
        let config = config;
        let layout = layout;
        let config_options = config_options;
        let opts = zellij_cli_args;
        let is_a_reconnect = false;

        //if let Some(reconnect_to_session) = &reconnect_to_session {
        //    // this is integration code to make session reconnects work with this existing,
        //    // untested and pretty involved function
        //    //
        //    // ideally, we should write tests for this whole function and refctor it
        //    if reconnect_to_session.name.is_some() {
        //        opts.command = Some(Command::Sessions(Sessions::Attach {
        //            session_name: reconnect_to_session.name.clone(),
        //            create: true,
        //            force_run_commands: false,
        //            index: None,
        //            options: None,
        //        }));
        //    } else {
        //        opts.command = None;
        //        opts.session = None;
        //        config_options.attach_to_session = None;
        //    }
        //    is_a_reconnect = true;
        //}

        if let Some(Command::Sessions(Sessions::Attach {
            session_name,
            create,
            force_run_commands,
            index,
            options,
        })) = opts.command.clone()
        {
            let config_options = match options.as_deref() {
                Some(SessionCommand::Options(o)) => {
                    config_options.merge_from_cli(o.to_owned().into())
                },
                None => config_options,
            };

            let client = if let Some(idx) = index {
                attach_with_session_index(config_options.clone(), idx, create)
            } else {
                let session_exists = session_name
                    .as_ref()
                    .and_then(|s| session_exists(s).ok())
                    .unwrap_or(false);
                let resurrection_layout =
                    session_name.as_ref().and_then(|s| resurrection_layout(s));
                //if create && !session_exists && resurrection_layout.is_none() {
                //    session_name.clone().map(start_client_plan);
                //}

                match (session_name.as_ref(), resurrection_layout) {
                    (Some(session_name), Some(mut resurrection_layout)) if !session_exists => {
                        if force_run_commands {
                            resurrection_layout.recursively_add_start_suspended(Some(false));
                        }
                        ClientInfo::Resurrect(session_name.clone(), resurrection_layout)
                    },
                    _ => attach_with_session_name(session_name, config_options.clone(), create),
                }
            };

            let attach_layout = match &client {
                ClientInfo::Attach(_, _) => None,
                ClientInfo::New(_) => Some(layout),
                ClientInfo::Resurrect(_session_name, layout_to_resurrect) => {
                    Some(layout_to_resurrect.clone())
                },
            };

            let tab_position_to_focus = reconnect_to_session
                .as_ref()
                .and_then(|r| r.tab_position);
            let pane_id_to_focus = reconnect_to_session
                .as_ref()
                .and_then(|r| r.pane_id);
            start_client_ssh(
                Box::new(os_input),
                opts,
                config,
                config_options,
                client,
                attach_layout,
                tab_position_to_focus,
                pane_id_to_focus,
                is_a_reconnect,
                pty,
            );
        } else if let Some(session_name) = opts.session.clone() {
            start_client_ssh(
                Box::new(os_input),
                opts,
                config,
                config_options,
                ClientInfo::New(session_name),
                Some(layout),
                None,
                None,
                is_a_reconnect,
                pty,
            );
        } else if let Some(session_name) = config_options.session_name.as_ref() {
            if let Ok(val) = envs::get_session_name() {
                // This prevents the same type of recursion as above, only that here we
                // don't get the command to "attach", but to start a new session instead.
                // This occurs for example when declaring the session name inside a layout
                // file and then, from within this session, trying to open a new zellij
                // session with the same layout. This causes an infinite recursion in the
                // `zellij_server::terminal_bytes::listen` task, flooding the server and
                // clients with infinite `Render` requests.
                if *session_name == val {
                    eprintln!("You are trying to attach to the current session (\"{session_name}\"). Zellij does not support nesting a session in itself.");
                    process::exit(1);
                }
            }
            match config_options.attach_to_session {
                Some(true) => {
                    let client = attach_with_session_name(
                        Some(session_name.clone()),
                        config_options.clone(),
                        true,
                    );
                    let attach_layout = match &client {
                        ClientInfo::Attach(_, _) => None,
                        ClientInfo::New(_) => Some(layout),
                        ClientInfo::Resurrect(_, resurrection_layout) => {
                            Some(resurrection_layout.clone())
                        },
                    };
                    start_client_ssh(
                        Box::new(os_input),
                        opts,
                        config,
                        config_options,
                        client,
                        attach_layout,
                        None,
                        None,
                        is_a_reconnect,
                        pty,
                    );
                },
                _ => {
                    start_client_ssh(
                        Box::new(os_input),
                        opts,
                        config,
                        config_options.clone(),
                        ClientInfo::New(session_name.clone()),
                        Some(layout),
                        None,
                        None,
                        is_a_reconnect,
                        pty,
                    );
                },
            }
            //if reconnect_to_session.is_some() {
            //    continue;
            //}
            // after we detach, this happens and so we need to exit before the rest of the
            // function happens
            //process::exit(0);
        }

        //}
    }
}
use zellij_client::ClientInfo;

fn attach_with_cli_client(
    cli_action: zellij_utils::cli::CliAction,
    session_name: &str,
    config: Option<Config>,
) {
    let os_input = get_os_input(zellij_client::os_input_output::get_client_os_input);
    let get_current_dir = || std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match Action::actions_from_cli(cli_action, Box::new(get_current_dir), config) {
        Ok(actions) => {
            zellij_client::cli_client::start_cli_client(Box::new(os_input), session_name, actions);
            std::process::exit(0);
        },
        Err(e) => {
            eprintln!("{e}");
            log::error!("Error sending action: {}", e);
            std::process::exit(2);
        },
    }
}

fn attach_with_session_index(config_options: Options, index: usize, create: bool) -> ClientInfo {
    // Ignore the session_name when `--index` is provided
    match get_sessions_sorted_by_mtime() {
        Ok(sessions) if sessions.is_empty() => {
            // if create {
            //     create_new_client()
            // } else {
            //     eprintln!("No active zellij sessions found.");
            //     process::exit(1);
            // }
            process::exit(1);
        },
        Ok(sessions) => find_indexed_session(sessions, config_options, index, create),
        Err(e) => {
            eprintln!("Error occurred: {e:?}");
            process::exit(1);
        },
    }
}

fn attach_with_session_name(
    session_name: Option<String>,
    config_options: Options,
    create: bool,
) -> ClientInfo {
    match &session_name {
        Some(session) if create => {
            if session_exists(session).unwrap() {
                ClientInfo::Attach(session_name.unwrap(), config_options)
            } else {
                ClientInfo::New(session_name.unwrap())
            }
        },
        Some(prefix) => match match_session_name(prefix).unwrap() {
            SessionNameMatch::UniquePrefix(s) | SessionNameMatch::Exact(s) => {
                ClientInfo::Attach(s, config_options)
            },
            SessionNameMatch::AmbiguousPrefix(_sessions) => {
                println!(
                    "Ambiguous selection: multiple sessions names start with '{prefix}':"
                );
                //print_sessions(
                //    sessions
                //        .iter()
                //        .map(|s| (s.clone(), Duration::default(), false))
                //        .collect(),
                //    false,
                //    false,
                //);
                process::exit(1);
            },
            SessionNameMatch::None => {
                eprintln!("No session with the name '{prefix}' found!");
                process::exit(1);
            },
        },
        None => match get_active_session() {
            //ActiveSession::None if create => create_new_client(),
            ActiveSession::None => {
                eprintln!("No active zellij sessions found.");
                process::exit(1);
            },
            ActiveSession::One(session_name) => ClientInfo::Attach(session_name, config_options),
            ActiveSession::Many => {
                println!("Please specify the session to attach to, either by using the full name or a unique prefix.\nThe following sessions are active:");
                list_sessions(false);
                process::exit(1);
            },
        },
    }
}

use crate::zellij_session::{print_sessions_with_index, resurrection_layout, session_exists};
use zellij_utils::{envs, nix};

pub fn get_os_input<OsInputOutput>(
    fn_get_os_input: fn() -> Result<OsInputOutput, nix::Error>,
) -> OsInputOutput {
    match fn_get_os_input() {
        Ok(os_input) => os_input,
        Err(e) => {
            eprintln!("failed to open terminal:\n{e}");
            process::exit(1);
        },
    }
}

fn get_server_input(
    handle: ServerHandle,
    channel_id: ChannelId,
    win_size: Winsize,
    sender: UnboundedSender<(Option<String>, Option<()>)>,
    server_receiver: crossbeam_channel::Receiver<Vec<u8>>,
    server_signal_receiver: crossbeam_channel::Receiver<Sig>,
) -> SshInputOutput {
    //let orig_termios = None; // not a terminal
    let reading_from_stdin = Arc::new(Mutex::new(None));
    SshInputOutput {
        handle,
        win_size,
        channel_id,
        sender,
        server_receiver,
        server_signal_receiver,
        send_instructions_to_server: Arc::new(Mutex::new(None)),
        receive_instructions_from_server: Arc::new(Mutex::new(None)),
        reading_from_stdin,
        session_name: Arc::new(Mutex::new(None)),
    }
}

fn find_indexed_session(
    sessions: Vec<String>,
    config_options: Options,
    index: usize,
    _create: bool,
) -> ClientInfo {
    match sessions.get(index) {
        Some(session) => ClientInfo::Attach(session.clone(), config_options),
        None => {
            println!(
                "No session indexed by {index} found. The following sessions are active:"
            );
            print_sessions_with_index(sessions);
            process::exit(1);
        },
    }
}

use nix::{
    pty::{openpty, OpenptyResult, Winsize},
};

fn handle_openpty() -> OpenptyResult {
    openpty(None, None).expect("Creating pty failed")
}
