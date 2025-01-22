use std::{env, path};
use tf2_demostats::{parser, schema};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().without_time().with_target(false))
        .with(EnvFilter::from_default_env())
        .init();

    let multiple_files = env::args().len() > 2;

    let schema_string = std::env::var("TF2_SCHEMA_PATH")
        .expect("TF2_SCHEMA_PATH must be set")
        .to_string();
    let schema_path = path::Path::new(&schema_string);
    let schema = schema::parse(schema_path)?;

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
