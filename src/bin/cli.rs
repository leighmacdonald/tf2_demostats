use std::{env, path};
use tf2_demostats::parser;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("debug"))
        .format_timestamp(None)
        .init();

    for arg in env::args().skip(1) {
        let path = path::Path::new(&arg);
        let bytes = tokio::fs::read(path).await?;
        let demo = parser::parse(&bytes).expect("Demo should parse");
        println!("{}", serde_json::to_string(&demo).unwrap());
    }

    Ok(())
}
