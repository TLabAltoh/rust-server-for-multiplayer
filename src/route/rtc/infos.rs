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
struct JSON {
    room_id: i32,
    room_key: String,
    user_id: i32,
    user_token: u32,
}

pub fn route() -> Router<AppState> {
    Router::new().route("/stream/infos/:base64/", post(infos))
}

async fn infos(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/infos");

    let json: JSON = match parse_base64_into_json(&params) {
        Ok(json) => json,
        Err(err_response) => return Ok(err_response),
    };

    let (room, client) = match auth_user(
        json.room_id.clone(),
        json.room_key.clone(),
        json.user_id,
        json.user_token,
    )
    .await
    {
        Ok((room, client)) => (room, client),
        Err(err_response) => return Ok(err_response),
    };

    if !client.check_token(json.user_token.clone()) {
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
    let json = serde_json::to_string(&infos).unwrap();
    return Ok(http::create_response(json.into(), StatusCode::OK));
}
