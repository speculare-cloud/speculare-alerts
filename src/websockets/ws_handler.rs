use std::io::{Error, ErrorKind};

use futures::StreamExt;
use sproot::Pool;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Error as TError, Message};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::CONFIG;

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
            "{}/ws?query={}:{}",
            &CONFIG.wss_domain, self.query, self.table
        );

        let mut request = ws_url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("SP-ADM", CONFIG.cdc_adm.clone().parse().unwrap());

        let ws_stream = connect_async(request)
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
