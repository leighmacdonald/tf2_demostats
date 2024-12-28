mod game;
pub mod summarizer;
mod weapon;

use summarizer::DemoSummary;
use tf_demo_parser::demo::header::Header;
use tf_demo_parser::{Demo, DemoParser};

pub fn parse(buffer: &[u8]) -> tf_demo_parser::Result<(Header, DemoSummary)> {
    let demo = Demo::new(&buffer);
    let handler = summarizer::MatchAnalyzer::new();
    let stream = demo.get_stream();
    let parser = DemoParser::new_with_analyser(stream, handler);

    parser.parse()
}
