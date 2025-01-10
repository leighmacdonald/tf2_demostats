use actix_multipart::form::tempfile::TempFileConfig;
use actix_multipart::form::MultipartFormConfig;
use actix_web::{error, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use std::{env, fs, path};
use tf2_demostats::{schema, web::handler};
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn handle_multipart_error(err: actix_multipart::MultipartError, _req: &HttpRequest) -> Error {
    let response = HttpResponse::BadRequest().force_close().finish();
    error::InternalError::from_response(err, response).into()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = env::var("DEMO_HOST").unwrap_or_else(|_e| String::from("0.0.0.0"));
    let port = env::var("DEMO_PORT")
        .unwrap_or_else(|_e| String::from("8811"))
        .parse()
        .unwrap();

    fs::create_dir_all("tmp")?;
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let schema_string = std::env::var("DEMO_TF2_SCHEMA_PATH")
        .or(std::env::var("TF2_SCHEMA_PATH"))
        .expect("DEMO_TF2_SCHEMA_PATH must be set")
        .to_string();
    let schema_path = path::Path::new(&schema_string);
    let schema = schema::parse(schema_path)?;
    info!("Loaded TF2 schema from {}", &schema_string);

    let schema_data = web::Data::new(schema);

    info!("Starting HTTP Service on {}:{}", &host, &port);
    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .app_data(schema_data.clone())
            .app_data(TempFileConfig::default().directory("tmp"))
            .app_data(
                MultipartFormConfig::default()
                    .total_limit(1000 * 1024 * 1024) // 1000 GB
                    .memory_limit(100 * 1024 * 1024) // 100 MB
                    .error_handler(handle_multipart_error),
            )
            .app_data(web::JsonConfig::default()
                // limit request payload size
                .limit(250_000))
            .service(
                web::resource("/")
                    .route(web::get().to(handler::index))
                    .route(web::post().to(handler::save_files)),
            )
    }).
        bind((host, port))?.
        //workers(4). // TODO Add env var
        run().
        await
}
