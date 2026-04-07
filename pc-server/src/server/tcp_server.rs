use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{error, info};

pub struct TcpServer {
    addr: String,
}

impl TcpServer {
    pub fn new(host: &str, port: u16) -> Self {
        TcpServer {
            addr: format!("{}:{}", host, port),
        }
    }

    pub async fn run<F, Fut>(&self, handler: F)
    where
        F: Fn(String) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = match TcpListener::bind(&self.addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind to {}: {}", self.addr, e);
                return;
            }
        };

        info!("TCP Server listening on {}", self.addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New connection from: {}", addr);

                    let handler = handler.clone();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stream);
                        let mut lines = reader.lines();

                        while let Ok(Some(line)) = lines.next_line().await {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                handler(trimmed.to_string()).await;
                            }
                        }
                        info!("Connection closed: {}", addr);
                    });
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }
    }
}