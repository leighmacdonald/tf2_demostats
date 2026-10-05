pub mod convert;
pub mod service;

// Generated `demostats.v1` message + service types (see `build.rs`).
// Machine output, not ours: silence lints for the whole blob.
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::restriction,
    clippy::cargo
)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/_connectrpc.rs"));
}
pub use generated::demostats;

use std::{path::Path, sync::Arc};

use tf2_demostats::schema;

/// Serve `DemoService` (Connect, gRPC, gRPC-Web) over HTTP.
///
/// # Errors
///
/// Returns an error if the schema cannot be read, the port cannot be bound,
/// or the server fails.
pub async fn serve(schema_path: &Path, host: String, port: u16) -> tf2_demostats::Result<()> {
    let schema = Arc::new(schema::read(schema_path)?);
    let app = service::DemoServiceImpl::router(schema).into_axum_router();

    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
