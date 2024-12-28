pub mod summarizer;
mod weapon;

use tf_demo_parser::demo::header::Header;
use tf_demo_parser::demo::parser::player_summary_analyzer::PlayerSummaryState;
use tf_demo_parser::{Demo, DemoParser};

pub fn parse(buffer: &[u8]) -> tf_demo_parser::Result<(Header, PlayerSummaryState)> {
    let demo = Demo::new(&buffer);
    let handler = summarizer::MatchAnalyzer::new();
    let stream = demo.get_stream();
    let parser = DemoParser::new_with_analyser(stream, handler);

    parser.parse()
}
