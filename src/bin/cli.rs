use std::{env, path};
use tf2_demostats::{parser, schema, Result};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
extern crate dotenv;

use dotenv::dotenv;

#[actix_web::main]
async fn main() -> Result<()> {
    dotenv().ok();
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .without_time()
                .with_target(false)
                .with_writer(std::io::stderr),
        )
        .with(EnvFilter::from_default_env())
        .init();

    let schema = schema::fetch().await?;

    let multiple_files = env::args().len() > 2;

    for arg in env::args().skip(1) {
        let path = path::Path::new(&arg);
        let bytes = tokio::fs::read(path).await?;

        let _span_guard = if multiple_files {
            Some(tracing::error_span!("Demo", "{}", arg).entered())
        } else {
            None
        };

        let mut demo = parser::parse(&bytes, &schema).expect("Demo should parse");
        demo.filename = Some(arg);
        println!("{}", serde_json::to_string(&demo).unwrap());
    }

    Ok(())
}
