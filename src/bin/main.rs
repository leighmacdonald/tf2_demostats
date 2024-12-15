use std::fs;
use actix_web::{web, App, HttpServer, middleware};
use actix_multipart::{form::{
    tempfile::{TempFileConfig},
}};
use tf2_demostats::web::handler;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = "[::1]";
    let port = 6969;
    fs::create_dir_all("./tmp")?;
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    log::info!("Starting HTTP Service on {}:{}", &host, &port);
    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .app_data(TempFileConfig::default().directory("./tmp"))
            .service(
                web::resource("/")
                    .route(web::get().to(handler::index))
                    .route(web::post().to(handler::save_files)),
            )
    }).
        bind((host, port))?.
        //workers(2).
        run().
        await
}
