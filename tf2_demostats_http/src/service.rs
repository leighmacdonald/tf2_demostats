//! `DemoService` implementation: parse a raw `.dem` upload, return the typed summary.

use std::sync::Arc;

use connectrpc::{
    ConnectError, ErrorCode, RequestContext, Response, Router, ServiceRequest, ServiceResult,
};

use crate::convert;
use crate::demostats::v1::{DemoService, ParseDemoRequest, ParseDemoResponse};

/// Reject uploads larger than this (demos in the wild are ~10-150MB).
pub const MAX_DEMO_BYTES: usize = 1024 * 1024 * 1024;

pub struct DemoServiceImpl {
    schema: Arc<tf2_demostats::schema::Schema>,
}

impl DemoServiceImpl {
    #[must_use]
    pub fn new(schema: Arc<tf2_demostats::schema::Schema>) -> Self {
        Self { schema }
    }

    #[must_use]
    pub fn router(schema: Arc<tf2_demostats::schema::Schema>) -> Router {
        Router::new()
            .add_service(Arc::new(Self::new(schema)))
            .with_route_limits(
                "/demostats.v1.DemoService/ParseDemo",
                connectrpc::Limits::default()
                    .with_max_request_body_size(MAX_DEMO_BYTES)
                    .with_max_message_size(MAX_DEMO_BYTES),
            )
    }
}

// The handler body is synchronous; `async` is part of the generated trait.
#[allow(
    refining_impl_trait_internal,
    refining_impl_trait_reachable,
    clippy::unused_async_trait_impl
)]
impl DemoService for DemoServiceImpl {
    async fn parse_demo(
        &self,
        _ctx: RequestContext,
        req: ServiceRequest<'_, ParseDemoRequest>,
    ) -> ServiceResult<ParseDemoResponse> {
        if req.demo.is_empty() {
            return Err(ConnectError::new(
                ErrorCode::InvalidArgument,
                "empty demo upload",
            ));
        }
        if req.demo.len() > MAX_DEMO_BYTES {
            return Err(ConnectError::new(
                ErrorCode::InvalidArgument,
                format!("demo exceeds {MAX_DEMO_BYTES} bytes"),
            ));
        }

        let output = tf2_demostats::parser::parse(req.demo, &self.schema).map_err(|e| {
            ConnectError::new(ErrorCode::InvalidArgument, format!("parse failed: {e}"))
        })?;

        let mut demo = convert::demo_output(&output);
        if !req.filename.is_empty() {
            demo.filename = req.filename.to_string();
        }

        Response::ok(ParseDemoResponse {
            demo: buffa::MessageField::some(demo),
            ..Default::default()
        })
    }
}
