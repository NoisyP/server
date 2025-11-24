use mysql::prelude::*;
use mysql::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
include!("settings.rs");

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

            // Crea tabelle se non esistono
            create_tables(&pool);

            let listener =
                TcpListener::bind("0.0.0.0:8080").expect("Errore nel binding della porta 8080");
            println!("Server HTTP in ascolto su http://127.0.0.1:8080");

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

    // Tabella users
    conn.query_drop(
        r"CREATE TABLE IF NOT EXISTS users (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(100) NOT NULL UNIQUE,
            age INT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .expect("Errore nella creazione della tabella users");

    // Tabella events
    conn.query_drop(
        r"CREATE TABLE IF NOT EXISTS events (
            uid INT AUTO_INCREMENT PRIMARY KEY,
            title VARCHAR(255) NOT NULL,
            description TEXT,
            date VARCHAR(50) NOT NULL,
            location VARCHAR(255) NOT NULL,
            image_url TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
            map_position DOUBLE,
        )",
    )
    .expect("Errore nella creazione della tabella events");

    println!("✓ Tabelle pronte");
}

fn handle_client(mut stream: std::net::TcpStream, pool: Pool) {
    let mut buffer = [0; 1024];

    if let Ok(_) = stream.read(&mut buffer) {
        let request = String::from_utf8_lossy(&buffer[..]);
        let lines: Vec<&str> = request.lines().collect();

        if lines.is_empty() {
            return;
        }

        let request_line = lines[0];
        println!("Richiesta: {}", request_line);

        let path = if let Some(path) = request_line.split(' ').nth(1) {
            path
        } else {
            "/"
        };

        let response = match path {
            // ============ ENDPOINTS USERS ============
            "/api/users" => {
                let users = get_all_users(&pool);
                let json = format!(
                    r#"{{"users": [{}]}}"#,
                    users
                        .iter()
                        .map(|u| format!(
                            r#"{{"id": {}, "name": "{}", "email": "{}", "age": {}}}"#,
                            u.0, u.1, u.2, u.3
                        ))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                create_json_response(&json)
            }

            path if path.starts_with("/api/add-user") => {
                if let Some(query) = path.split('?').nth(1) {
                    let params = parse_query(query);

                    if let (Some(name), Some(email)) = (params.get("name"), params.get("email")) {
                        let age: i32 = params.get("age").and_then(|a| a.parse().ok()).unwrap_or(0);

                        add_user(&pool, name, email, age);
                        create_json_response(
                            r#"{"status": "success", "message": "Utente aggiunto"}"#,
                        )
                    } else {
                        create_json_response(
                            r#"{"status": "error", "message": "Parametri mancanti"}"#,
                        )
                    }
                } else {
                    create_json_response(
                        r#"{"status": "error", "message": "Query string richiesta"}"#,
                    )
                }
            }

            path if path.starts_with("/api/user/") => {
                if let Ok(id) = path.replace("/api/user/", "").parse::<i32>() {
                    if let Some(user) = get_user_by_id(&pool, id) {
                        let json = format!(
                            r#"{{"id": {}, "name": "{}", "email": "{}", "age": {}}}"#,
                            user.0, user.1, user.2, user.3
                        );
                        create_json_response(&json)
                    } else {
                        create_json_response(
                            r#"{"status": "error", "message": "Utente non trovato"}"#,
                        )
                    }
                } else {
                    create_json_response(r#"{"status": "error", "message": "ID invalido"}"#)
                }
            }

            "/api/delete-user" => {
                if let Some(query) = path.split('?').nth(1) {
                    let params = parse_query(query);
                    if let Some(id_str) = params.get("id") {
                        if let Ok(id) = id_str.parse::<i32>() {
                            delete_user(&pool, id);
                            create_json_response(
                                r#"{"status": "success", "message": "Utente eliminato"}"#,
                            )
                        } else {
                            create_json_response(r#"{"status": "error", "message": "ID invalido"}"#)
                        }
                    } else {
                        create_json_response(r#"{"status": "error", "message": "ID richiesto"}"#)
                    }
                } else {
                    create_json_response(
                        r#"{"status": "error", "message": "Query string richiesta"}"#,
                    )
                }
            }

            // ============ ENDPOINTS EVENTS ============
            "/api/events" => {
                let events = get_all_events(&pool);
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
            }

            path if path.starts_with("/api/event/") => {
                if let Ok(uid) = path.replace("/api/event/", "").parse::<i32>() {
                    if let Some(event) = get_event_by_id(&pool, uid) {
                        let json = format!(
                            r#"{{"uid": {}, "title": "{}", "description": "{}", "date": "{}", "location": "{}", "imageUrl": "{}"}}"#,
                            event.0,
                            escape_json(&event.1),
                            escape_json(&event.2),
                            event.3,
                            escape_json(&event.4),
                            escape_json(&event.5)
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

            path if path.starts_with("/api/add-event") => {
                if let Some(query) = path.split('?').nth(1) {
                    let params = parse_query(query);

                    if let (Some(title), Some(date), Some(location)) = (
                        params.get("title"),
                        params.get("date"),
                        params.get("location"),
                    ) {
                        let description =
                            params.get("description").map(|s| s.as_str()).unwrap_or("");
                        let image_url = params.get("imageUrl").map(|s| s.as_str()).unwrap_or("");

                        add_event(&pool, title, description, date, location, image_url);
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
            }

            "/api/delete-event" => {
                if let Some(query) = path.split('?').nth(1) {
                    let params = parse_query(query);
                    if let Some(uid_str) = params.get("uid") {
                        if let Ok(uid) = uid_str.parse::<i32>() {
                            delete_event(&pool, uid);
                            create_json_response(
                                r#"{"status": "success", "message": "Evento eliminato"}"#,
                            )
                        } else {
                            create_json_response(
                                r#"{"status": "error", "message": "UID invalido"}"#,
                            )
                        }
                    } else {
                        create_json_response(r#"{"status": "error", "message": "UID richiesto"}"#)
                    }
                } else {
                    create_json_response(
                        r#"{"status": "error", "message": "Query string richiesta"}"#,
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

// ============ FUNZIONI USERS ============

fn get_all_users(pool: &Pool) -> Vec<(i32, String, String, i32)> {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    conn.query_map(
        "SELECT id, name, email, age FROM users",
        |(id, name, email, age)| (id, name, email, age),
    )
    .unwrap_or_default()
}

fn get_user_by_id(pool: &Pool, id: i32) -> Option<(i32, String, String, i32)> {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    conn.exec_first::<(i32, String, String, i32), _, _>(
        "SELECT id, name, email, age FROM users WHERE id = ?",
        (id,),
    )
    .ok()
    .flatten()
}

fn add_user(pool: &Pool, name: &str, email: &str, age: i32) {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    let _ = conn.exec_drop(
        "INSERT INTO users (name, email, age) VALUES (?, ?, ?)",
        (name, email, age),
    );
    println!("✓ Utente aggiunto: {} ({})", name, email);
}

fn delete_user(pool: &Pool, id: i32) {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    let _ = conn.exec_drop("DELETE FROM users WHERE id = ?", (id,));
    println!("✓ Utente eliminato: ID {}", id);
}

// ============ FUNZIONI EVENTS ============

fn get_all_events(pool: &Pool) -> Vec<(i32, String, String, String, String, String, String)> {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    conn.query_map(
        "SELECT uid, title, description, date, location, image_url, map_position FROM events ORDER BY uid DESC",
        |(uid, title, description, date, location, image_url, map_position)| {
            (
                uid,
                title,
                description,
                date,
                location,
                image_url,
                map_position,
            )
        },
    )
    .unwrap_or_default()
}

fn get_event_by_id(pool: &Pool, uid: i32) -> Option<(i32, String, String, String, String, String)> {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    conn.exec_first::<(i32, String, String, String, String, String, String), _, _>(
        "SELECT uid, title, description, date, location, image_url, map_position FROM events WHERE uid = ?",
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
) {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    let _ = conn.exec_drop(
        "INSERT INTO events (title, description, date, location, image_url) VALUES (?, ?, ?, ?, ?)",
        (title, description, date, location, image_url),
    );
    println!("✓ Evento aggiunto: {} ({})", title, date);
}

fn delete_event(pool: &Pool, uid: i32) {
    let mut conn = pool.get_conn().expect("Errore nella connessione");
    let _ = conn.exec_drop("DELETE FROM events WHERE uid = ?", (uid,));
    println!("✓ Evento eliminato: UID {}", uid);
}
