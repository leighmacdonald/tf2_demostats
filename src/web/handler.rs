use crate::parser;
use actix_multipart::form::tempfile::TempFile;
use actix_multipart::form::MultipartForm;
use actix_web::{HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::io::Read;
use tf_demo_parser::demo::header::Header;
use tf_demo_parser::demo::parser::player_summary_analyzer::PlayerSummaryState;
use tf_demo_parser::{Demo, DemoParser};

#[derive(Debug, MultipartForm)]
pub struct UploadForm {
    #[multipart(rename = "file")]
    files: Vec<TempFile>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct DemoDetail {
    state: PlayerSummaryState,
    header: Header,
}

pub async fn save_files(MultipartForm(form): MultipartForm<UploadForm>) -> impl Responder {
    for mut f in form.files {
        let path = f.file_name.unwrap();
        println!("Reading file: {:?}", path);

        let mut buffer = Vec::new();

        match f.file.read_to_end(&mut buffer) {
            Ok(f) => f,
            Err(e) => {
                log::error!("Failed to read upload {:?}", e);
                return HttpResponse::InternalServerError().body(e.to_string());
            }
        };
        let demo = Demo::new(&buffer);
        let handler = parser::summarizer::MatchAnalyzer::new();
        let stream = demo.get_stream();
        let parser = DemoParser::new_with_analyser(stream, handler);

        let (header, state) = match parser.parse() {
            Ok((h, s)) => (h, s),
            Err(e) => {
                log::error!("Failed to parse upload {:?}", e);
                return HttpResponse::BadRequest().body(e.to_string());
            }
        };

        // println!("{:?}", header);
        // println!("{:?}", state);

        let detail = DemoDetail { header, state };

        return HttpResponse::Ok().json(detail);
    }

    println!("Invalid request");

    HttpResponse::BadRequest().finish()
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
