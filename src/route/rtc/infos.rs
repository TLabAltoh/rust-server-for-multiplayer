use axum::body::Body;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use http::response::StreamInfo;
use http::BodyUtil;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tracing::debug;

use crate::http;
use crate::result::Result;
use crate::route::*;
use crate::AppState;

#[derive(Serialize, Deserialize)]
struct RequestJson {
    room_id: i32,
    user_id: i32,
    token: u32,
    shared_key: String,
}

pub fn route() -> Router<AppState> {
    Router::new().route("/stream/infos/:base64/", post(infos))
}

async fn infos(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/infos");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let (room, client) = match auth_user(
        request.room_id.clone(),
        request.shared_key.clone(),
        request.user_id,
        request.token,
    )
    .await
    {
        Ok((room, client)) => (room, client),
        Err(err_response) => return Ok(err_response),
    };

    if !client.check_token(request.token.clone()) {
        return Ok(http::create_response(
            Body::from(BodyUtil::INVILED_TOKEN),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let streams = client.get_streams().await;
    let forwarder = room.forwarder();
    let forwarder = forwarder.write().await;
    let infos: Vec<StreamInfo> = forwarder
        .forward_infos(streams)
        .await
        .into_iter()
        .map(|forward_info| forward_info.into())
        .collect();
    drop(forwarder);
    return Ok(http::create_response(
        serde_json::to_string(&infos).unwrap().into(),
        StatusCode::OK,
    ));
}
