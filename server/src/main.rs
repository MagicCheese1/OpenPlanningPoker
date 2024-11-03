use cookie::{Cookie, CookieJar};
use serde::{Deserialize, Serialize};
use warp::{Filter, Rejection};
use uuid::Uuid;
use warp::http::Response;
use warp_sessions::{CookieOptions, MemoryStore, SameSiteCookieOption, SessionWithStore};

#[derive(Deserialize, Serialize)]
pub struct Username {
    value: String,
}

impl Username {
    pub fn new(username: &str) -> Result<Username, &'static str> {
        if Username::is_valid(username) {
            Ok(Username { value: username.to_string() })
        } else {
            Err("Invalid username: it must be between 3 and 20 characters long and contain only alphanumeric characters")
        }
    }
    fn is_valid(username: &str) -> bool {
        let len = username.len();
        len >= 3 && len <= 20 && username.chars().all(char::is_alphanumeric)
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}
#[derive(Deserialize, Serialize)]
pub struct User {
    uuid: Uuid,
    name: Username,
    current_table_id: Option<String>,
}

impl User {
    fn new(name: String) -> Result<User, &'static str> {
        let uuid = Uuid::new_v4();
        let username = Username::new(&name)?;

        Ok(User {
            uuid,
            name: username,
            current_table_id: None,
        })
    }
}

fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}

#[tokio::main]
async fn main() {
    let session_filter = warp::any()
        .map(|| {
            let session_id = generate_session_id();
            let mut jar = CookieJar::new();
            jar.add(Cookie::new("session_id", session_id.clone()));

            Response::builder()
                .header("set-cookie", jar.to_string())
                .body(format!("Your session ID: {}", session_id))
        });

    warp::serve(session_filter).run(([127, 0, 0, 1], 3030)).await;
}