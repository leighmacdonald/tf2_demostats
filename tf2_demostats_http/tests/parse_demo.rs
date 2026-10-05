//! End-to-end coverage for `DemoService::ParseDemo` using the repo fixtures.
//!
//! Integration tests run with CWD set to the crate root, so fixtures live
//! at `../test.dem` / `../schema.json`.

use std::sync::Arc;

use buffa::Message;
use buffa::view::HasMessageView;
use connectrpc::{CodecFormat, Encodable, ErrorCode, RequestContext, ServiceRequest};
use tf2_demostats_http::demostats::v1::{DemoService, ParseDemoRequest, ParseDemoResponse};
use tf2_demostats_http::service::DemoServiceImpl;

async fn parse_fixture() -> tf2_demostats::parser::DemoOutput {
    let schema = tf2_demostats::schema::read(std::path::Path::new("../schema.json"))
        .await
        .expect("schema.json should load");
    let bytes = tokio::fs::read("../test.dem")
        .await
        .expect("test.dem should load");
    tf2_demostats::parser::parse(&bytes, &schema).expect("test.dem should parse")
}

/// The typed proto conversion preserves everything the JSON API reports.
#[tokio::test]
async fn convert_matches_json_api() {
    let output = parse_fixture().await;
    let json = serde_json::to_value(&output).expect("output serializes");
    let proto = tf2_demostats_http::convert::demo_output(&output);

    let header = &proto.header.as_option().expect("header set");
    assert_eq!(header.map, json["map"].as_str().unwrap());
    assert_eq!(header.game, json["game"].as_str().unwrap());
    assert_eq!(header.ticks, json["ticks"].as_u64().unwrap() as u32);
    assert_eq!(header.frames, json["frames"].as_u64().unwrap() as u32);

    let summary = proto.summary.as_option().expect("summary set");
    assert_eq!(
        summary.rounds.len(),
        json["rounds"].as_array().unwrap().len()
    );
    assert_eq!(summary.chat.len(), json["chat"].as_array().unwrap().len());

    for (round, round_json) in summary
        .rounds
        .iter()
        .zip(json["rounds"].as_array().unwrap())
    {
        assert_eq!(
            round.players.len(),
            round_json["players"].as_array().unwrap().len()
        );
        for (player, player_json) in round
            .players
            .iter()
            .zip(round_json["players"].as_array().unwrap())
        {
            assert_eq!(player.name, player_json["name"].as_str().unwrap_or(""));
            assert_eq!(player.steamid, player_json["steamid"].as_str().unwrap());
            let stats = player.stats.as_option().expect("stats set");
            assert_eq!(
                stats.kills,
                player_json["kills"].as_u64().unwrap_or(0) as u32
            );
            assert_eq!(
                stats.deaths,
                player_json["deaths"].as_u64().unwrap_or(0) as u32
            );
            assert_eq!(
                stats.damage,
                player_json["damage"].as_u64().unwrap_or(0) as u32
            );
            assert_eq!(
                player.weapons.len(),
                player_json["weapons"].as_object().map_or(0, serde_json::Map::len)
            );
        }
    }

    for (msg, msg_json) in summary.chat.iter().zip(json["chat"].as_array().unwrap()) {
        assert_eq!(msg.tick, msg_json["tick"].as_u64().unwrap() as u32);
        assert_eq!(msg.message, msg_json["message"].as_str().unwrap());
    }
}

/// The handler parses a real upload and echoes the request filename.
#[tokio::test]
async fn handler_parses_demo() {
    let schema = tf2_demostats::schema::read(std::path::Path::new("../schema.json"))
        .await
        .expect("schema.json should load");
    let svc = DemoServiceImpl::new(Arc::new(schema));
    let demo = tokio::fs::read("../test.dem")
        .await
        .expect("test.dem should load");

    let body = buffa::bytes::Bytes::from(
        ParseDemoRequest {
            demo,
            filename: "test.dem".into(),
            ..Default::default()
        }
        .encode_to_vec(),
    );
    let view = ParseDemoRequest::decode_view(&body).expect("request decodes");
    let req = ServiceRequest::<ParseDemoRequest>::from_parts(&view, &body);

    let resp = svc
        .parse_demo(RequestContext::new(http::HeaderMap::new()), req)
        .await
        .expect("parse succeeds");

    let bytes = Encodable::encode(&resp.body, CodecFormat::Proto).expect("encodes");
    let reply = ParseDemoResponse::decode_from_slice(&bytes).expect("decodes");
    let demo = reply.demo.as_option().expect("demo set");
    assert_eq!(demo.filename, "test.dem");
    let summary = demo.summary.as_option().expect("summary set");
    assert_ne!(summary.rounds, [] as [tf2_demostats_http::demostats::v1::RoundSummary; 0]);
    assert!(
        summary.rounds.iter().any(|r| !r.players.is_empty()),
        "at least one round has players"
    );
}

/// Empty and garbage uploads are rejected with `InvalidArgument`.
#[tokio::test]
async fn handler_rejects_bad_uploads() {
    let schema = tf2_demostats::schema::read(std::path::Path::new("../schema.json"))
        .await
        .expect("schema.json should load");
    let svc = DemoServiceImpl::new(Arc::new(schema));

    for demo in [Vec::new(), b"not a demo".to_vec()] {
        let body = buffa::bytes::Bytes::from(
            ParseDemoRequest {
                demo,
                ..Default::default()
            }
            .encode_to_vec(),
        );
        let view = ParseDemoRequest::decode_view(&body).expect("request decodes");
        let req = ServiceRequest::<ParseDemoRequest>::from_parts(&view, &body);
        let err = svc
            .parse_demo(RequestContext::new(http::HeaderMap::new()), req)
            .await
            .expect_err("upload rejected");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
    }
}
