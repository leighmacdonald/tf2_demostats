use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use std::{
    env,
    fs::File,
    io,
    path::{Path, PathBuf},
    process::ExitCode,
};
use tf2_demostats::{
    Result, parser,
    schema::{self, download_schema},
};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};
extern crate dotenv;

use dotenv::dotenv;

const DEFAULT_SCHEMA: &str = "schema.json";

#[derive(Parser, Debug)]
#[command(name = "completion-derive")]
struct Cli {
    #[arg(long = "generate", value_enum)]
    generator: Option<clap_complete::Shell>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "version information")]
    Version,

    #[command(about = "parse demo")]
    Parse {
        #[arg(short, long, default_value = DEFAULT_SCHEMA)]
        schema: PathBuf,

        #[arg(required=true, value_hint = ValueHint::FilePath, num_args = 1..)]
        demo: Vec<PathBuf>,
    },
    #[command(about = "Update the local schema")]
    Update {
        #[arg(short, long, default_value = DEFAULT_SCHEMA)]
        schema: PathBuf,

        #[arg(
            short,
            long,
            help = "Steam Web API key. See: https://steamcommunity.com/dev/apikey"
        )]
        api_key: String,
    },
    #[command(about = "Start HTTP server")]
    Serve {
        #[arg(short, long, default_value = DEFAULT_SCHEMA)]
        schema: PathBuf,

        #[arg(short('H'), long, default_value = "0.0.0.0")]
        host: String,

        #[arg(short, long, default_value = "8811")]
        port: u16,
    },
}

#[actix_web::main]
async fn main() -> ExitCode {
    dotenv().ok();

    match exec().await {
        Err(e) => {
            error!("Error: {}", e);
            return ExitCode::FAILURE;
        }
        Ok(_) => ExitCode::SUCCESS,
    }
}
async fn exec() -> Result<()> {
    let args = Cli::parse();
    if let Some(generator) = args.generator {
        let mut cmd = Cli::command();
        print_completions(generator, &mut cmd);
        return Ok(());
    }

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .without_time()
                .with_target(false)
                .with_writer(std::io::stderr),
        )
        .with(EnvFilter::from_default_env())
        .init();
    match args.command {
        Commands::Version => cmd_version().await,
        Commands::Parse { schema, demo } => cmd_parse(&schema, demo).await,
        Commands::Update {
            schema,
            api_key: key,
        } => cmd_schema(key, &schema).await,
        Commands::Serve { schema, host, port } => cmd_serve(&schema, host, port).await,
    }
}

fn print_completions<G: clap_complete::Generator>(generator: G, cmd: &mut clap::Command) {
    clap_complete::generate(
        generator,
        cmd,
        cmd.get_name().to_string(),
        &mut io::stdout(),
    );
}

async fn cmd_serve(schema_path: &Path, host: String, port: u16) -> Result<()> {
    tf2_demostats_http::serve(schema_path, host, port).await?;

    Ok(())
}

async fn cmd_parse(schema_path: &Path, demo_paths: Vec<PathBuf>) -> Result<()> {
    let schema = schema::read(schema_path).await?;

    for mut demo_path in demo_paths {
        let path = demo_path.as_path();
        let bytes = tokio::fs::read(path).await?;

        let mut demo = parser::parse(&bytes, &schema).expect("Demo should parse");
        demo.filename = Some(String::from(demo_path.to_str().unwrap()));
        demo_path.add_extension("json");
        let mut out_file = File::create(demo_path)?;
        serde_json::to_writer(&mut out_file, &demo)?;
    }

    Ok(())
}

async fn cmd_version() -> Result<()> {
    println!("tf2_demostats {}", env!("CARGO_PKG_VERSION"));

    Ok(())
}

async fn cmd_schema(api_key: String, schema_path: &Path) -> Result<()> {
    download_schema(api_key, schema_path).await?;
    info!("Schema downloaded successfully: {}", schema_path.display());

    Ok(())
}
