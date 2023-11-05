mod handler;
mod session;
mod server;
mod zellij_session;
mod ssh;

use std::fmt::{Debug, Display, Formatter};
use std::io::{Error, ErrorKind};

use russh::{Pty, ChannelId, server::Handle, CryptoVec};

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
        //let (tx, rx) = crossbeam_channel::bounded(1);
        //let runtime_handle = self.runtime_handle.clone();
        //let handle = self.handle.clone();
        //let channel_id = self.channel_id.clone();
        //let data = CryptoVec::from_slice(buf);
        self.sender.send((Some(String::from_utf8_lossy(buf).to_string()), None));
        //runtime_handle.spawn(async move {
        //    let res = handle.data(channel_id, data).await;
        //    //let _ = tx.send(res);
        //});

        //let res = rx.recv().map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;
        //res.map_err(|e|Error::new(ErrorKind::Other,"handle data"))?;
        Ok(buf.len())
    }
   fn write_all(&mut self, mut buf: &[u8]) -> std::io::Result<()> {
        self.write(buf).map(|_|())
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
use zellij_utils::logging::configure_logger;
use zellij_server_command::CliArgs;
use zellij_utils::clap::Parser;

#[tokio::main]
async fn main() {
    configure_logger();
    let opts = CliArgs::parse();
    let s = server::Server::new(opts);
    s.listen().await;
}