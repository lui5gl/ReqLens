pub mod forward;
pub mod handler;

use crate::config::cli::AppConfig;
use crate::error::Result;
use crate::ingest::IngestSender;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::future::Future;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};

pub async fn run_server(
    config: Arc<AppConfig>,
    ingest: IngestSender,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    let listener = TcpListener::bind(config.listen_addr).await?;
    info!("ReqLens listening on http://{}", config.listen_addr);
    info!("Forwarding upstream traffic to {}", config.upstream_uri);

    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("Shutdown signal received in proxy listener.");
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((stream, client_addr)) => {
                        let io = TokioIo::new(stream);
                        let cfg = Arc::clone(&config);
                        let ing = ingest.clone();

                        tokio::spawn(async move {
                            let service = service_fn(move |req| {
                                handler::handle_request(req, client_addr, Arc::clone(&cfg), ing.clone())
                            });

                            if let Err(err) = hyper::server::conn::http1::Builder::new()
                                .serve_connection(io, service)
                                .await
                            {
                                error!("Error serving client connection {}: {:?}", client_addr, err);
                            }
                        });
                    }
                    Err(err) => {
                        error!("Accept error on listener: {}", err);
                    }
                }
            }
        }
    }

    Ok(())
}
