use crate::{parser, schema::Schema};
use actix_multipart::form::{tempfile::TempFile, MultipartForm};
use actix_web::{web, HttpResponse, Responder};
use std::io::Read;
use tracing::error;

#[derive(Debug, MultipartForm)]
pub struct UploadForm {
    #[multipart(rename = "file")]
    file: TempFile,
}

pub async fn save_files(
    MultipartForm(mut form): MultipartForm<UploadForm>,
    schema: web::Data<Schema>,
) -> impl Responder {
    let mut buffer = Vec::new();

    match form.file.file.read_to_end(&mut buffer) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to read upload {:?}", e);
            return HttpResponse::BadRequest().body(e.to_string());
        }
    };

    let mut output = match parser::parse(&buffer, &schema) {
        Ok(o) => o,
        Err(e) => {
            error!("Failed to parse upload {:?}", e);
            return HttpResponse::BadRequest().body(e.to_string());
        }
    };

    output.filename = form.file.file_name;

    HttpResponse::Ok().json(output)
}

pub async fn index() -> HttpResponse {
    let html = r#"<html>
        <head><title>Upload Test</title></head>
        <body>
            <form target="/" method="post" enctype="multipart/form-data">
                <input type="file" multiple name="file"/>
                <button type="submit">Submit</button>
            </form>
        </body>
    </html>"#;

    HttpResponse::Ok().body(html)
}
