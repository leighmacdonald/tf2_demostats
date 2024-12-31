use std::{env, path};
use tf2_demostats::parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().without_time().with_target(false))
        .with(EnvFilter::from_default_env())
        .init();

    let multiple_files = env::args().len() > 2;

    for arg in env::args().skip(1) {
        let path = path::Path::new(&arg);
        let bytes = tokio::fs::read(path).await?;

        let _span_guard = if multiple_files {
            Some(tracing::info_span!("Demo", "File={}", arg).entered())
        } else {
            None
        };

        let mut demo = parser::parse(&bytes).expect("Demo should parse");
        demo.filename = Some(arg);
        println!("{}", serde_json::to_string(&demo).unwrap());
    }

    Ok(())
}
