use dotenv::dotenv;
use once_cell::sync::Lazy;
use std::env;

#[derive(Debug)]
pub struct Environment {
    pub server_port: String,
    pub jwt_secret: String,
    pub server_environment: String,
    pub open_search_dns: String,
    pub database_url: String,
}

static ENVIRONMENTS: Lazy<Environment> = Lazy::new(|| {
    dotenv().ok();
    Environment {
        server_port: env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string()),
        jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| "".to_string()),
        server_environment: env::var("SERVER_ENVIRONMENT")
            .unwrap_or_else(|_| "development".to_string()),
        open_search_dns: env::var("OPEN_SEARCH_DNS")
            .unwrap_or_else(|_| "http://localhost:9200".to_string()),
        database_url: env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://sqlite3.db".to_string()),
    }
});

pub fn get_environments() -> &'static Environment {
    &ENVIRONMENTS
}
