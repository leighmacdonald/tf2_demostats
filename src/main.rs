use fnv::{FnvHashMap, FnvHashSet};
use std::fs;
use actix_web::{get, web, App, Error, HttpServer, Responder, HttpResponse, middleware};
use actix_multipart::{form::{
    tempfile::{TempFile, TempFileConfig},
    MultipartForm,
}};
use tf_demo_parser::demo::data::DemoTick;
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::packet::datatable::{ParseSendTable, SendTableName, ServerClass};
use tf_demo_parser::demo::parser::{DemoHandler, MessageHandler, RawPacketStream};
use tf_demo_parser::demo::sendprop::{SendPropIdentifier, SendPropName};
use tf_demo_parser::{Demo, MessageType, ParseError, ParserState};
use tf_demo_parser::demo::header::Header;
use bitbuffer::BitRead;

#[get("/test/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    format!("Hello {name}!")
}

#[derive(Debug, MultipartForm)]
struct UploadForm {
    #[multipart(rename = "file")]
    files: Vec<TempFile>,
}


#[derive(Default)]
struct PropAnalyzer {
    props: FnvHashSet<SendPropIdentifier>,
    prop_names: FnvHashMap<SendPropIdentifier, (SendTableName, SendPropName)>,
}

impl MessageHandler for PropAnalyzer {
    type Output = Vec<String>;

    fn does_handle(message_type: MessageType) -> bool {
        matches!(message_type, MessageType::PacketEntities)
    }

    fn handle_message(&mut self, message: &Message, _tick: DemoTick, _parser_state: &ParserState) {
        if let Message::PacketEntities(message) = message {
            for entity in &message.entities {
                for prop in &entity.props {
                    self.props.insert(prop.identifier);
                }
            }
        }
    }

    fn handle_data_tables(
        &mut self,
        parse_tables: &[ParseSendTable],
        _server_classes: &[ServerClass],
        _parser_state: &ParserState,
    ) {
        for table in parse_tables {
            for prop_def in &table.props {
                self.prop_names.insert(
                    prop_def.identifier(),
                    (table.name.clone(), prop_def.name.clone()),
                );
            }
        }
    }

    fn into_output(self, _state: &ParserState) -> Self::Output {
        let names = self.prop_names;
        let mut props = self
            .props
            .into_iter()
            .map(|prop| {
                let (table, name) = names.get(&prop).unwrap();
                format!("{}.{}", table, name)
            })
            .collect::<Vec<_>>();
        props.sort();
        props
    }
}

fn parse(file: &Vec<u8>) -> Result<(), ParseError> {
    let demo = Demo::new(&file);
    let mut handler = DemoHandler::default();
    let mut stream = demo.get_stream();

    // let header = Header::read(&mut stream)?;
    // handler.handle_header(&header);

    let mut packets = RawPacketStream::new(stream);
    while let Some(packet) = packets.next(&handler.state_handler)? {
        println!("{:?}", &packet);
        let packet = handler.handle_packet(packet).unwrap();
    }

    assert_eq!(false, packets.incomplete);

    return Ok(());
}

async fn save_files(
    MultipartForm(form): MultipartForm<UploadForm>,
) -> Result<impl Responder, Error> {
    for f in form.files {
        let path = f.file_name.unwrap();
        let file = fs::read(path)?;
let config_max = Some(77u8);
        let resp = match parse(&file) {
            Ok(i) => Ok(HttpResponse::Ok()),
            Err(e) => Err(HttpResponse::BadRequest()),
        };
        let x = Some('x');
        return Ok(resp);
    };

    Ok(HttpResponse::Ok())
}

async fn index() -> HttpResponse {
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = "127.0.0.1";
    let port = 6969;
    fs::create_dir_all("./tmp")?;
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    log::info!("Starting HTTP Service on {}:{}", host, port);
    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(TempFileConfig::default().directory("./tmp"))
            .service(
                web::resource("/")
                    .route(web::get().to(index))
                    .route(web::post().to(save_files)),
            )
    }).
        bind((host, port))?.

        workers(2).
        run().
        await
}
