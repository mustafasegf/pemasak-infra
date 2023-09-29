use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
};

use byte_unit::Byte;
use config::{Config, ConfigError};
use serde::Deserialize;
use sqlx::postgres::PgConnectOptions;

#[derive(Deserialize, Debug, Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application: ApplicationSettings,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApplicationSettings {
    pub port: u16,
    pub host: String,
    pub body_limit: String,
    pub secret: String,
    pub auth: bool,
    pub ipv6: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DatabaseSettings {
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub name: String,
    pub timeout: u64,
}

pub fn get_configuration() -> Result<Settings, ConfigError> {
    Config::builder()
        .set_default("application.host", "127.0.0.1")?
        .set_default("application.port", 8080)?
        .set_default("application.body_limit", "0")?
        .set_default("application.auth", true)?
        .set_default("application.ipv6", false)?
        .set_default("database.user", "postgres")?
        .set_default("database.password", "postgres")?
        .set_default("database.host", "127.0.0.1")?
        .set_default("database.port", 5432)?
        .set_default("database.name", "postgres")?
        .set_default("database.timeout", 20)?
        .add_source(config::Environment::default().separator("_"))
        .add_source(config::File::with_name("configuration"))
        .build()?
        .try_deserialize::<Settings>()
}

impl Settings {
    pub fn connection_options(&self) -> PgConnectOptions {
        PgConnectOptions::new()
            .host(&self.database.host)
            .port(self.database.port)
            .username(&self.database.user)
            .password(&self.database.password)
            .database(&self.database.name)
    }

    pub fn address_string(&self) -> String {
        format!("{}:{}", self.application.host, self.application.port)
    }

    pub fn address(&self) -> io::Result<SocketAddr> {
        self.address_string()
            .to_socket_addrs()?
            .min_by_key(|addr| match addr {
                SocketAddr::V4(_) => self.application.ipv6 as usize,
                SocketAddr::V6(_) => self.application.ipv6 as usize ^ 1,
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "invalid address"))
    }

    pub fn body_limit(&self) -> usize {
        Byte::from_str(&self.application.body_limit)
            .unwrap_or(Byte::from_bytes(0))
            .get_bytes() as usize
    }
}