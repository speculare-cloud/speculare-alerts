use crate::CONFIG;

use futures::StreamExt;
use sproot::Pool;
use std::io::{Error, ErrorKind};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Error::{AlreadyClosed, ConnectionClosed, Io as TIo};
use tokio_tungstenite::tungstenite::{Error as TError, Message};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct WsHandler<'a> {
    pub query: &'a str,
    pub table: &'a str,
    pub msg_error: fn(TError) -> std::io::Result<()>,
    pub msg_ok: fn(Message, &Pool),
    pub pool: &'a Pool,
}

impl<'a> WsHandler<'a> {
    async fn open_connection(&self) -> std::io::Result<WsStream> {
        let ws_url = format!(
            "wss://{}/ws?query={}:{}",
            &CONFIG.wss_domain, self.query, self.table
        );

        let ws_stream = connect_async(&ws_url)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, e))?
            .0;

        Ok(ws_stream)
    }

    pub async fn listen(&self) -> std::io::Result<()> {
        let mut ws_stream = self.open_connection().await?;

		trace!("listening on the websocket");
        // While we have some message, read them and wait for the next one
        while let Some(msg) = ws_stream.next().await {
            match msg {
                Err(err) => {
                    (self.msg_error)(err)?;
                }
                Ok(msg) if msg.is_text() => {
                    (self.msg_ok)(msg, self.pool);
                }
                _ => {}
            }
        }

        Ok(())
    }
}

pub fn msg_err_handler(err: TError) -> std::io::Result<()> {
    match err {
        // Consider those kind of error as fatal
        ConnectionClosed | AlreadyClosed | TIo(_) => {
            error!("WebSocket: error is fatal: {}", err);
            Err(Error::new(ErrorKind::Other, err))
        }
        _ => {
            debug!("WebSocket: error is non-fatal: {}", err);
            Ok(())
        }
    }
}
