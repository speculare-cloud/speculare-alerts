use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// Helper method that connect to the WS passed as URL and return the Stream
pub async fn connect_to_ws(url: &str) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Error> {
    match connect_async(url).await {
        Ok(val) => {
            debug!("Websocket: {} handshake completed", url);
            Ok(val.0)
        }
        Err(err) => {
            error!("Websocket: error while connecting {}: \"{}\"", url, err);
            Err(err)
        }
    }
}
