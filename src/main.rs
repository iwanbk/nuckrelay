mod handshake;
mod message;
mod peer;
mod server;
mod store;

use clap::Parser;
use tokio::net::TcpListener;
use tracing::info;

use crate::server::Server;

#[derive(Parser, Debug)]
#[clap(name = "nuckrelay", about = "Nuck relay server")]
struct Cli {
    /// exposed address
    #[clap(long, env = "NB_EXPOSED_ADDRESS")]
    exposed_address: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create the server
    let server = Server::new(cli.exposed_address, false)?;

    let try_socket = TcpListener::bind("127.0.0.1:9000").await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", listener.local_addr().unwrap());

    while let Ok((stream, addr)) = listener.accept().await {
        let server_clone = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server_clone.handle_connection(stream, addr).await {
                info!("Error handling connection: {}", e);
            }
        });
    }

    Ok(())
}
