// settings.rs
pub struct DatabaseConfig {
    pub user: String,
    pub password: String,
    pub ip_address: String,
    pub port: String,
}

pub fn get_db_info() -> Result<DatabaseConfig, String> {
    Ok(DatabaseConfig {
        user: "root".to_string(),
        password: "password123".to_string(),
        ip_address: "192.168.64.4".to_string(), // IP del tuo container
        port: "3306".to_string(),
    })
}
