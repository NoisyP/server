use mysql::prelude::*;
use mysql::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

// Struttura per la configurazione del database
pub struct DatabaseConfig {
    pub user: String,
    pub password: String,
    pub ip_address: String,
    pub port: String,
}

// Funzione per ottenere le info del database
pub fn get_db_info() -> Result<DatabaseConfig, String> {
    Ok(DatabaseConfig {
        user: "tonight_user".to_string(),
        password: "password123".to_string(),
        ip_address: "localhost".to_string(),
        port: "3306".to_string(),
    })
}

fn main() {
    println!("Avvio del server HTTP con MySQL...");

    match get_db_info() {
        Ok(db) => {
            let url = format!(
                "mysql://{}:{}@{}:{}/tonight",
                db.user, db.password, db.ip_address, db.port,
            );
            let opts = Opts::from_url(&url).expect("URL non valido");
            let pool = Pool::new(opts).expect("Errore nella connessione a MySQL");

            create_tables(&pool);

            let listener =
                TcpListener::bind("0.0.0.0:8080").expect("Errore nel binding della porta 8080");
            println!("Server HTTP in ascolto su http://0.0.0.0:8080");

            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let pool_clone = pool.clone();
                        thread::spawn(move || {
                            handle_client(stream, pool_clone);
                        });
                    }
                    Err(e) => {
                        println!("Errore nella connessione: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            println!("Error fetching database information: {}", e);
            return;
        }
    }
}

fn create_tables(pool: &Pool) {
    let mut conn = pool.get_conn().expect("Errore nella connessione al pool");

    // Tabella users con cloudflare_id
    conn.query_drop(
        r"CREATE TABLE IF NOT EXISTS users (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(100) NOT NULL UNIQUE,
            age INT,
            cloudflare_id VARCHAR(255) UNIQUE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .expect("Errore nella creazione della tabella users");

    // Tabella events con user_id
    conn.query_drop(
        r"CREATE TABLE IF NOT EXISTS events (
            uid INT AUTO_INCREMENT PRIMARY KEY,
            title VARCHAR(255) NOT NULL,
            description TEXT,
            date VARCHAR(50) NOT NULL,
            location VARCHAR(255) NOT NULL,
            image_url TEXT,
            map_position VARCHAR(255),
            user_id INT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )",
    )
    .expect("Errore nella creazione della tabella events");

    println!("✓ Tabelle pronte");
}

fn handle_client(mut stream: std::net::TcpStream, pool: Pool) {
    let mut buffer = [0; 2048];

    if let Ok(_) = stream.read(&mut buffer) {
        let request = String::from_utf8_lossy(&buffer[..]);
        let lines: Vec<&str> = request.lines().collect();

        if lines.is_empty() {
            return;
        }

        let request_line = lines[0];
        println!("Richiesta: {}", request_line);

        // Estrai cloudflare_id dall'header Authorization
        let cloudflare_id = extract_auth_header(&lines);

        let path = if let Some(path) = request_line.split(' ').nth(1) {
            path
        } else {
            "/"
        };

        let response = match path {
            // ============ AUTH ENDPOINTS ============
            "/api/auth/register" => {
                if let Some(query) = path.split('?').nth(1) {
                    let params = parse_query(query);
                    if let (Some(name), Some(email), Some(cf_id)) = (
                        params.get("name"),
                        params.get("email"),
                        params.get("cloudflare_id"),
                    ) {
                        let age: i32 = params.get("age").and_then(|a| a.parse().ok()).unwrap_or(0);
                        match register_user(&pool, name, email, age, cf_id) {
                            Ok(_) => create_json_response(
                                r#"{"status": "success", "message": "Utente registrato"}"#,
                            ),
                            Err(e) => create_json_response(&format!(
                                r#"{{"status": "error", "message": "{}"}}"#,
                                e
                            )),
                        }
                    } else {
                        create_json_response(
                            r#"{"status": "error", "message": "Parametri mancanti (name, email, cloudflare_id)"}"#,
                        )
                    }
                } else {
                    create_json_response(
                        r#"{"status": "error", "message": "Query string richiesta"}"#,
                    )
                }
            }

            "/api/auth/me" => {
                if let Some(cf_id) = &cloudflare_id {
                    if let Some(user) = get_user_by_cloudflare_id(&pool, cf_id) {
                        let json = format!(
                            r#"{{"id": {}, "name": "{}", "email": "{}", "age": {}, "cloudflare_id": "{}"}}"#,
                            user.0, user.1, user.2, user.3, user.4
                        );
                        create_json_response(&json)
                    } else {
                        create_json_response(
                            r#"{"status": "error", "message": "Utente non trovato"}"#,
                        )
                    }
                } else {
                    create_json_response(
                        r#"{"status": "error", "message": "Authorization header mancante"}"#,
                    )
                }
            }

            // ============ ENDPOINTS EVENTS ============
            // GET tutti gli eventi (PUBBLICO - no auth)
            "/api/events" => {
                let events = get_all_events(&pool);
                let json = format!(
                    r#"{{"events": [{}]}}"#,
                    events
                        .iter()
                        .map(|e| format!(
                            r#"{{"uid": {}, "title": "{}", "description": "{}", "date": "{}", "location": "{}", "imageUrl": "{}", "map_position": "{}", "user_id": {}}}"#,
                            e.0, escape_json(&e.1), escape_json(&e.2), e.3, escape_json(&e.4), escape_json(&e.5), escape_json(&e.6), e.7,
                        ))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                create_json_response(&json)
            }

            // GET i miei eventi (RICHIEDE AUTH)
            "/api/my-events" => {
                if let Some(cf_id) = &cloudflare_id {
                    if let Some(user) = get_user_by_cloudflare_id(&pool, cf_id) {
                        let events = get_events_by_user(&pool, user.0);
                        let json = format!(
                            r#"{{"events": [{}]}}"#,
                            events
                                .iter()
                                .map(|e| format!(
                                    r#"{{"uid": {}, "title": "{}", "description": "{}", "date": "{}", "location": "{}", "imageUrl": "{}", "map_position": "{}"}}"#,
                                    e.0, escape_json(&e.1), escape_json(&e.2), e.3, escape_json(&e.4), escape_json(&e.5), escape_json(&e.6),
                                ))
                                .collect::<Vec<_>>()
                                .join(",")
                        );
                        create_json_response(&json)
                    } else {
                        create_json_response(
                            r#"{"status": "error", "message": "Utente non trovato"}"#,
                        )
                    }
                } else {
                    create_json_response(
                        r#"{"status": "error", "message": "Authorization header mancante"}"#,
                    )
                }
            }

            // GET singolo evento (PUBBLICO - no auth)
            path if path.starts_with("/api/event/") => {
                if let Ok(uid) = path.replace("/api/event/", "").parse::<i32>() {
                    if let Some(event) = get_event_by_id(&pool, uid) {
                        let json = format!(
                            r#"{{"uid": {}, "title": "{}", "description": "{}", "date": "{}", "location": "{}", "imageUrl": "{}", "map_position": "{}", "user_id": {}}}"#,
                            event.0,
                            escape_json(&event.1),
                            escape_json(&event.2),
                            event.3,
                            escape_json(&event.4),
                            escape_json(&event.5),
                            escape_json(&event.6),
                            event.7
                        );
                        create_json_response(&json)
                    } else {
                        create_json_response(
                            r#"{"status": "error", "message": "Evento non trovato"}"#,
                        )
                    }
                } else {
                    create_json_response(r#"{"status": "error", "message": "UID invalido"}"#)
                }
            }

            // POST aggiungi evento (RICHIEDE AUTH)
            path if path.starts_with("/api/add-event") => {
                if let Some(cf_id) = &cloudflare_id {
                    if let Some(user) = get_user_by_cloudflare_id(&pool, cf_id) {
                        if let Some(query) = path.split('?').nth(1) {
                            let params = parse_query(query);

                            if let (Some(title), Some(date), Some(location)) = (
                                params.get("title"),
                                params.get("date"),
                                params.get("location"),
                            ) {
                                let description =
                                    params.get("description").map(|s| s.as_str()).unwrap_or("");
                                let image_url =
                                    params.get("imageUrl").map(|s| s.as_str()).unwrap_or("");

                                add_event(&pool, title, description, date, location, image_url, user.0);
                                create_json_response(
                                    r#"{"status": "success", "message": "Evento aggiunto"}"#,
                                )
                            } else {
                                create_json_response(
                                    r#"{"status": "error", "message": "Parametri mancanti (title, date, location)"}"#,
                                )
                            }
                        } else {
                            create_json_response(
                                r#"{"status": "error", "message": "Query string richiesta"}"#,
                            )
                        }
                    } else {
                        create_json_response(
                            r#"{"status": "error", "message": "Utente non trovato"}"#,
                        )
                    }
                } else {
                    create_json_response(
                        r#"{"status": "error", "message": "Authorization header mancante"}"#,
                    )
                }
            }

            // DELETE evento (RICHIEDE AUTH - solo proprietario)
            "/api/delete-event" => {
                if let Some(cf_id) = &cloudflare_id {
                    if let Some(user) = get_user_by_cloudflare_id(&pool, cf_id) {
                        if let Some(query) = path.split('?').nth(1) {
                            let params = parse_query(query);
                            if let Some(uid_str) = params.get("uid") {
                                if let Ok(uid) = uid_str.parse::<i32>() {
                                    if can_delete_event(&pool, uid, user.0) {
                                        delete_event(&pool, uid);
                                        create_json_response(
                                            r#"{"status": "success", "message": "Evento eliminato"}"#,
                                        )
                                    } else {
                                        create_json_response(
                                            r#"{"status": "error", "message": "Non autorizzato"}"#,
                                        )
                                    }
                                } else {
                                    create_json_response(
                                        r#"{"status": "error", "message": "UID invalido"}"#,
                                    )
                                }
                            } else {
                                create_json_response(
                                    r#"{"status": "error", "message": "UID richiesto"}"#,
                                )
                            }
                        } else {
                            create_json_response(
                                r#"{"status": "error", "message": "Query string richiesta"}"#,
                            )
                        }
                    } else {
                        create_json_response(
                            r#"{"status": "error", "message": "Utente non trovato"}"#,
                        )
                    }
                } else {
                    create_json_response(
                        r#"{"status": "error", "message": "Authorization header mancante"}"#,
                    )
                }
            }

            _ => create_json_response(r#"{"status": "error", "message": "Endpoint non trovato"}"#),
        };

        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }
}

// ============ FUNZIONI HELPER ============

fn extract_auth_header(lines: &[&str]) -> Option<String> {
    for line in lines {
        if line.to_lowercase().starts_with("authorization:") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                return Some(parts[1].trim().to_string());
            }
        }
    }
    None
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in query.split('&') {
        let parts: Vec<&str> = pair.split('=').collect();
        if parts.len() == 2 {
            map.insert(parts[0].to_string(), parts[1].to_string());
        }
    }
    map
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn create_json_response(json: &str) -> String {
    let content_length = json.len();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        content_length, json
    )
}

// ============ FUNZIONI AUTH ============

fn register_user(
    pool: &Pool,
    name: &str,
    email: &str,
    age: i32,
    cloudflare_id: &str,
) -> Result<(), String> {
    let mut conn = pool.get_conn().map_err(|e| e.to_string())?;
    conn.exec_drop(
        "INSERT INTO users (name, email, age, cloudflare_id) VALUES (?, ?, ?, ?)",
        (name, email, age, cloudflare_id),
    )
    .map_err(|e| e.to_string())?;
    println!("✓ Utente registrato: {} ({})", name, email);
    Ok(())
}

fn get_user_by_cloudflare_id(pool: &Pool, cloudflare_id: &str) -> Option<(i32, String, String, i32, String)> {
    let mut conn = pool.get_conn().ok()?;
    conn.exec_first::<(i32, String, String, i32, String), _, _>(
        "SELECT id, name, email, age, cloudflare_id FROM users WHERE cloudflare_id = ?",
        (cloudflare_id,),
    )
    .ok()
    .flatten()
}

// ============ FUNZIONI EVENTS ============

fn get_all_events(pool: &Pool) -> Vec<(i32, String, String, String, String, String, String, i32)> {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    conn.query_map(
        "SELECT uid, title, description, date, location, image_url, map_position, user_id FROM events ORDER BY uid DESC",
        |(uid, title, description, date, location, image_url, map_position, user_id)| {
            (uid, title, description, date, location, image_url, map_position, user_id)
        },
    )
    .unwrap_or_default()
}

fn get_events_by_user(pool: &Pool, user_id: i32) -> Vec<(i32, String, String, String, String, String, String)> {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    conn.exec_map(
        "SELECT uid, title, description, date, location, image_url, map_position FROM events WHERE user_id = ? ORDER BY uid DESC",
        (user_id,),
        |(uid, title, description, date, location, image_url, map_position)| {
            (uid, title, description, date, location, image_url, map_position)
        },
    )
    .unwrap_or_default()
}

fn get_event_by_id(pool: &Pool, uid: i32) -> Option<(i32, String, String, String, String, String, String, i32)> {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    conn.exec_first::<(i32, String, String, String, String, String, String, i32), _, _>(
        "SELECT uid, title, description, date, location, image_url, map_position, user_id FROM events WHERE uid = ?",
        (uid,),
    )
    .ok()
    .flatten()
}

fn add_event(
    pool: &Pool,
    title: &str,
    description: &str,
    date: &str,
    location: &str,
    image_url: &str,
    user_id: i32,
) {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    let _ = conn.exec_drop(
        "INSERT INTO events (title, description, date, location, image_url, user_id) VALUES (?, ?, ?, ?, ?, ?)",
        (title, description, date, location, image_url, user_id),
    );
    println!("✓ Evento aggiunto: {} ({})", title, date);
}

fn delete_event(pool: &Pool, uid: i32) {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    let _ = conn.exec_drop("DELETE FROM events WHERE uid = ?", (uid,));
    println!("✓ Evento eliminato: UID {}", uid);
}

fn can_delete_event(pool: &Pool, event_uid: i32, user_id: i32) -> bool {
    if let Some(event) = get_event_by_id(pool, event_uid) {
        event.7 == user_id
    } else {
        false
    }
}
