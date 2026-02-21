pub mod handler;
use actix_multipart::form::MultipartFormConfig;
use actix_multipart::form::tempfile::TempFileConfig;
use actix_web::{App, Error, HttpRequest, HttpResponse, HttpServer, error, middleware, web};
use std::{fs, path::Path};
use tf2_demostats::{Result, schema};

fn handle_multipart_error(err: actix_multipart::MultipartError, _req: &HttpRequest) -> Error {
    let response = HttpResponse::BadRequest().force_close().finish();
    error::InternalError::from_response(err, response).into()
}

pub async fn serve(schema_path: &Path, host: String, port: u16) -> Result<()> {
    fs::create_dir_all("tmp")?;

    let schema = schema::read(schema_path).await?;
    let schema_data = web::Data::new(schema);

    let _ = HttpServer::new(move || {
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
        await;

    Ok(())
}
