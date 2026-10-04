pub mod convert;
pub mod service;

// Generated `demostats.v1` message + service types (see `build.rs`).
connectrpc::include_generated!();

use std::{path::Path, sync::Arc};

use tf2_demostats::schema;

/// Serve `DemoService` (Connect, gRPC, gRPC-Web) over HTTP.
pub async fn serve(schema_path: &Path, host: String, port: u16) -> tf2_demostats::Result<()> {
    let schema = Arc::new(schema::read(schema_path).await?);
    let app = service::DemoServiceImpl::router(schema).into_axum_router();

    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
