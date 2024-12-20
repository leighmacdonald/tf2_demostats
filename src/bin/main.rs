use std::{env, fs};
use actix_web::{web, App, HttpServer, middleware, error, HttpResponse, HttpRequest, Error};
use actix_multipart::form::{
    tempfile::{TempFileConfig},
};
use actix_multipart::form::MultipartFormConfig;
use tf2_demostats::web::handler;


fn handle_multipart_error(err: actix_multipart::MultipartError, _req: &HttpRequest) -> Error {
    let response = HttpResponse::BadRequest()
        .force_close()
        .finish();
    error::InternalError::from_response(err, response).into()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = env::var("DEMO_HOST").unwrap_or_else(|_e| String::from("0.0.0.0"));
    let port = env::var("DEMO_PORT").unwrap_or_else(|_e| String::from("8811")).parse().unwrap();

    fs::create_dir_all("tmp")?;
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    log::info!("Starting HTTP Service on {}:{}", &host, &port);

    HttpServer::new(|| {
        let json_cfg = web::JsonConfig::default()
            // limit request payload size
            .limit(250_000);

        App::new()
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .app_data(TempFileConfig::default().directory("tmp"))
            .app_data(
                MultipartFormConfig::default()
                    .total_limit(350 * 1024 * 1024) // 350 MB
                    .memory_limit(100 * 1024 * 1024) // 100 MB
                    .error_handler(handle_multipart_error),
            )
            .app_data(json_cfg)// 250mb
            .service(
                web::resource("/")
                    .route(web::get().to(handler::index))
                    .route(web::post().to(handler::save_files)),
            )
    }).
        bind((host, port))?.
        //workers(4).
        run().
        await
}
