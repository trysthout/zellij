mod handler;
mod server;
mod session;
mod ssh;
mod zellij_session;

use std::fmt::{Debug, Display, Formatter};


use russh::{server::Handle, ChannelId, Pty};

#[derive(Clone)]
pub struct ServerHandle(Handle);

impl Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HandleWrapper")
    }
}

#[derive(Clone)]
pub struct ServerOutput {
    sender: UnboundedSender<(Option<String>, Option<()>)>,
    handle: Handle,
    channel_id: ChannelId,
    runtime_handle: tokio::runtime::Handle,
}

impl std::io::Write for ServerOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.len() > 0 {
            let _ = self.sender
                .send((Some(String::from_utf8_lossy(buf).to_string()), None));
        } else {
            let _ = self.sender.send((None, Some(())));
        }
        
        Ok(buf.len())
    }
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.write(buf).map(|_| ())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct PtyRequest {
    pub term: String,
    pub col_width: u32,
    pub row_height: u32,
    pub pix_width: u32,
    pub pix_height: u32,
    pub modes: Vec<(Pty, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq)]
pub struct ServerChannelId(pub ChannelId);

impl Display for ServerChannelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

use tokio::sync::mpsc::UnboundedSender;
use zellij_server_command::CliArgs;
use zellij_utils::clap::Parser;
use zellij_utils::logging::configure_logger;

#[tokio::main]
async fn main() {
    configure_logger();
    let opts = CliArgs::parse();
    let s = server::Server::new(opts);
    s.listen().await;
}
