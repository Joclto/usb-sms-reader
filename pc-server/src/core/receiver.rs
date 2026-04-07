use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

pub struct MessageReceiver;

impl MessageReceiver {
    pub async fn listen<F, Fut>(addr: &str, handler: F)
    where
        F: Fn(String) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind to {}: {}", addr, e);
                return;
            }
        };

        info!("Message receiver listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    info!("New connection from: {}", peer_addr);

                    let handler = handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, handler).await {
                            error!("Connection error from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }
    }

    async fn handle_connection<F, Fut>(
        stream: TcpStream,
        handler: F,
    ) -> std::io::Result<()>
    where
        F: Fn(String) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let reader = BufReader::new(stream);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                handler(trimmed.to_string()).await;
            }
        }

        Ok(())
    }
}