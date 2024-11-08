use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time;
use uuid::Uuid;
use warp::{http::{Response, StatusCode, header}, Filter, Rejection, Reply};
use warp::cookie::optional;

#[derive(Deserialize, Serialize, Clone)]
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
#[derive(Deserialize, Serialize, Clone)]
pub struct User {
    uuid: Uuid,
    name: Username,
    current_table_id: Option<String>,
}

impl User {
    pub fn new(name: String) -> Result<User, &'static str> {
        let uuid = Uuid::new_v4();
        let username = Username::new(&name)?;

        Ok(User {
            uuid,
            name: username,
            current_table_id: None,
        })
    }

    pub fn user_id(&self) -> &Uuid {
        &self.uuid
    }

    pub fn name(&self) -> &Username {
        &self.name
    }
}

type UserStore = Arc<Mutex<HashMap<Uuid, User>>>;
type SessionStore = Arc<Mutex<HashMap<Uuid, Session>>>;
#[derive(Deserialize)]
struct NewSessionRequest {
    username: String,
}

pub struct Session {
    session_id: Uuid,
    user_id: Uuid,
    expires_at: u64,
}

impl Session {
    pub fn new(user_id: Uuid, duration: Duration) -> Session {
        let session_id = Uuid::new_v4();
        let expires_at = (SystemTime::now() + duration).duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        Session { session_id, user_id, expires_at }
    }

    pub fn session_id(&self) -> &Uuid {
        &self.session_id
    }

    pub fn user_id(&self) -> &Uuid {
        &self.user_id
    }

    pub fn is_expired(&self) -> bool {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() > self.expires_at
    }
}
async fn create_session_handler(new_session_req: NewSessionRequest, user_store: UserStore, session_store: SessionStore) -> Result<impl Reply, Rejection> {
    let user = match User::new(new_session_req.username) {
        Ok(v) => v,
        Err(e) => return Ok(Response::builder().status(StatusCode::BAD_REQUEST).body(e.to_string()).unwrap()),
    };
    let mut user_store = user_store.lock().unwrap();
    let user_id = user.user_id();
    user_store.insert(user_id.clone(), user.clone());

    let session = Session::new(user_id.clone(), Duration::from_secs(30));

    let mut session_store = session_store.lock().unwrap();
    let session_id = session.session_id();

    let cookie = format!("session_id={}; Secure; SameSite=Strict; HttpOnly; Path=/", session_id);
    session_store.insert(session_id.clone(), session);

    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header(header::SET_COOKIE, cookie)
        .body(format!("Session created for user: {}", user.name.value()))
        .unwrap();

    Ok(response)
}

async fn clean_up_sessions(session_store: SessionStore, user_store: UserStore) {
    let mut interval = time::interval(Duration::from_secs(86400)); // Run every 24 Hours

    loop {
        interval.tick().await;

        let mut session_store = session_store.lock().unwrap();
        let mut user_store = user_store.lock().unwrap();

        let expired_user_ids: Vec<Uuid> = session_store.iter()
            .filter_map(|(session_id, session)| {
                if session.is_expired() {
                    Some(session.user_id().clone())
                } else {
                    None
                }
            })
            .collect();

        session_store.retain(|_, session| !session.is_expired());

        for user_id in expired_user_ids {
            user_store.remove(&user_id);
        }
    }
}

async fn get_user_info_handler(session_id: Option<Uuid>, user_store: UserStore, session_store: SessionStore) -> Result<impl warp::Reply, Rejection> {
    if let Some(session_id) = session_id {
        let session_store = session_store.lock().unwrap();
        if let Some(session) = session_store.get(&session_id) {
            let user_store = user_store.lock().unwrap();
            if let Some(user) = user_store.get(session.user_id()) {
                return Ok(warp::reply::json(&user));
            }
        }
    }
    Err(warp::reject::not_found())
}

fn extract_session_id() -> impl Filter<Extract=(Option<Uuid>,), Error=Infallible> + Clone {
    optional("session_id").map(|session_id: Option<String>| {
        session_id.and_then(|id_str| Uuid::parse_str(&id_str).ok())
    })
}

#[tokio::main]
async fn main() {
    let user_store: UserStore = Arc::new(Mutex::new(HashMap::new()));
    let session_store: SessionStore = Arc::new(Mutex::new(HashMap::new()));

    tokio::spawn(clean_up_sessions(session_store.clone(), user_store.clone()));

    let user_store_filter = warp::any().map(move || user_store.clone());
    let session_store_filter = warp::any().map(move || session_store.clone());

    let create_session_route = warp::path("session")
        .and(warp::post())
        .and(warp::body::json())
        .and(user_store_filter.clone())
        .and(session_store_filter.clone())
        .and_then(create_session_handler);

    let get_user_info_route =
        warp::path("session")
            .and(warp::get())
            .and(extract_session_id())
            .and(user_store_filter.clone())
            .and(session_store_filter.clone())
            .and_then(get_user_info_handler);

    let routes = create_session_route.or(get_user_info_route).with(warp::log("sessions"));

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

