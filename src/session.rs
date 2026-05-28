use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

lazy_static! {
    static ref SESSION_STORE: SessionStore = SessionStore::new();
}

struct SessionStore {
    sessions: Mutex<HashMap<String, HashMap<String, String>>>,
    access_times: Mutex<HashMap<String, Instant>>,
}

impl SessionStore {
    fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            access_times: Mutex::new(HashMap::new()),
        }
    }

    fn create_session(&self) -> String {
        let session_id = self.generate_session_id();
        let now = Instant::now();
        let mut sessions = self.sessions.lock();
        let mut access_times = self.access_times.lock();
        sessions.insert(session_id.clone(), HashMap::new());
        access_times.insert(session_id.clone(), now);
        session_id
    }

    fn get(&self, session_id: &str, key: &str) -> Option<String> {
        let sessions = self.sessions.lock();
        let mut access_times = self.access_times.lock();
        if let Some(data) = sessions.get(session_id) {
            access_times.insert(session_id.to_string(), Instant::now());
            data.get(key).cloned()
        } else {
            None
        }
    }

    fn insert(&self, session_id: &str, key: &str, value: String) {
        let mut sessions = self.sessions.lock();
        let mut access_times = self.access_times.lock();
        if let Some(data) = sessions.get_mut(session_id) {
            data.insert(key.to_string(), value);
            access_times.insert(session_id.to_string(), Instant::now());
        }
    }

    fn remove(&self, session_id: &str, key: &str) -> Option<String> {
        let mut sessions = self.sessions.lock();
        let mut access_times = self.access_times.lock();
        if let Some(data) = sessions.get_mut(session_id) {
            access_times.insert(session_id.to_string(), Instant::now());
            data.remove(key)
        } else {
            None
        }
    }

    fn generate_session_id(&self) -> String {
        use std::time::SystemTime;
        let duration = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let timestamp = duration.as_nanos();
        let random = rand::random::<u64>();
        format!("{:x}{:x}", timestamp, random)
    }

    fn cleanup_expired(&self, max_age: Duration) {
        let now = Instant::now();
        let mut sessions = self.sessions.lock();
        let mut access_times = self.access_times.lock();
        access_times.retain(|session_id, last_access| {
            if now.duration_since(*last_access) >= max_age {
                sessions.remove(session_id);
                false
            } else {
                true
            }
        });
    }
}

#[derive(Clone)]
pub struct Session {
    session_id: String,
}

impl Session {
    pub fn new(session_id: Option<&str>) -> Self {
        let id = if let Some(sid) = session_id {
            let sessions = SESSION_STORE.sessions.lock();
            if sessions.contains_key(sid) {
                drop(sessions);
                sid.to_string()
            } else {
                drop(sessions);
                SESSION_STORE.create_session()
            }
        } else {
            SESSION_STORE.create_session()
        };
        Self { session_id: id }
    }

    pub fn id(&self) -> &str {
        &self.session_id
    }

    pub fn get(&self, key: &str) -> Option<String> {
        SESSION_STORE.get(&self.session_id, key)
    }

    pub fn insert(&self, key: &str, value: String) {
        SESSION_STORE.insert(&self.session_id, key, value);
    }

    pub fn remove(&self, key: &str) -> Option<String> {
        SESSION_STORE.remove(&self.session_id, key)
    }

    pub fn cookie_value(&self) -> String {
        format!("session_id={}; Path=/; HttpOnly", self.session_id)
    }
}

pub fn get_session_from_headers(headers: &HashMap<String, String>) -> Session {
    let session_id = headers.get("Cookie").and_then(|cookie| {
        cookie.split(';').find_map(|pair| {
            let trimmed = pair.trim();
            if trimmed.starts_with("session_id=") {
                Some(trimmed[11..].to_string())
            } else {
                None
            }
        })
    });
    Session::new(session_id.as_deref())
}

pub fn cleanup_sessions(max_age: Duration) {
    SESSION_STORE.cleanup_expired(max_age);
}
