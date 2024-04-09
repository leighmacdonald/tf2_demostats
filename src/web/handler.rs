use std::fs;
use actix_multipart::form::MultipartForm;
use actix_multipart::form::tempfile::TempFile;
use actix_web::{get, HttpResponse, Responder, web};
use tf_demo_parser::{Demo, DemoParser};
use crate::parser;

#[get("/test/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    format!("Hello {name}!")
}

#[derive(Debug, MultipartForm)]
pub struct UploadForm {
    #[multipart(rename = "file")]
    files: Vec<TempFile>,
}


pub async fn save_files(
    MultipartForm(form): MultipartForm<UploadForm>,
) -> impl Responder {
    for f in form.files {
        let path = f.file_name.unwrap();
        let file = match fs::read(path) {
            Ok(f) => f,
            Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
        };
        let demo = Demo::new(&file);
        let handler = parser::summarizer::MatchSummarizer::new();
        let stream = demo.get_stream();
        let parser = DemoParser::new_with_analyser(stream, handler);

        let (header, _state) = match parser.parse() {
            Ok((h, s)) => (h, s),
            Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
        };
        println!("{:?}", header);

        return HttpResponse::Ok().finish()
    };

    HttpResponse::Ok().finish()
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
