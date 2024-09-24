use axum::body::Body;
use axum::response::Response;
pub use http::StatusCode;

pub mod request;
pub mod response;

pub struct BodyUtil {}

impl BodyUtil {
    pub const SUCCEED: &'static str = "Succeed";
    pub const REJECTED: &'static str = "Rejected";
    pub const INVILED_TOKEN: &'static str = "Token Inviled";
    pub const INVILED_PASSWORD: &'static str = "Password Inviled";
    pub const ROOM_ID_NOTFOUND: &'static str = "Room ID Not Found";
    pub const UNKNOWN_ERROR: &'static str = "Unknown Error";
}

pub fn create_response(body: Body, status_code: StatusCode) -> Response<Body> {
    Response::builder().status(status_code).body(body).unwrap()
}
