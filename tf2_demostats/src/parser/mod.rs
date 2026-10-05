mod entity;
mod game;
pub mod player;
mod props;
pub mod stats;
pub mod summarizer;
mod weapon;

use crate::schema::Schema;
use serde::{Deserialize, Serialize};
use summarizer::DemoSummary;
use tf_demo_parser::{Demo, DemoParser, demo::header::Header};

#[derive(Debug, Serialize, Deserialize)]
pub struct DemoOutput {
    pub filename: Option<String>,

    #[serde(flatten)]
    pub header: Header,

    #[serde(flatten)]
    pub summary: DemoSummary,
}

/// Parse a raw `.dem` buffer into a [`DemoOutput`].
///
/// # Errors
///
/// Returns an error if the buffer is not a valid demo or parsing fails.
pub fn parse(buffer: &[u8], schema: &Schema) -> tf_demo_parser::Result<DemoOutput> {
    let demo = Demo::new(buffer);
    let handler = summarizer::MatchAnalyzer::new(schema);
    let stream = demo.get_stream();
    let parser = DemoParser::new_with_analyser(stream, handler);

    let (header, summary) = parser.parse()?;
    Ok(DemoOutput {
        header,
        summary,
        filename: None,
    })
}

// Helpers for serde serialization
#[must_use]
pub fn is_zero(num: &u32) -> bool {
    *num == 0
}

#[must_use]
pub fn is_false(b: &bool) -> bool {
    !(*b)
}
