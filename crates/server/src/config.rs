//! Server configuration and command-line parsing.
//!
//! Parsing is kept separate from bootstrap so the accepted flags can be tested
//! without opening a database or binding a socket.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

/// Fully resolved server configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    pub demo_seed: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 47_800,
            data_dir: PathBuf::from("data"),
            demo_seed: true,
        }
    }
}

/// Outcome of parsing the process arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Config),
    Help,
}

/// Reasons the command line could not be accepted.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("--host needs an IP address")]
    MissingHost,
    #[error("invalid host address {value}")]
    InvalidHost { value: String },
}

impl Config {
    /// Parses arguments after the executable name.
    ///
    /// The accepted surface is intentionally identical to the original inline
    /// parser in `main.rs`: `--host`, `--port`, `--data`, `--no-demo`, `--help`.
    /// An invalid host is an error; an invalid port keeps the previous value,
    /// matching the historical behavior.
    pub fn parse<I, S>(args: I) -> Result<Parsed, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut config = Config::default();
        let mut args = args.into_iter().map(Into::into);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--port" => {
                    if let Some(value) = args.next().and_then(|value| value.parse::<u16>().ok()) {
                        config.port = value;
                    }
                }
                "--host" => {
                    let Some(value) = args.next() else {
                        return Err(ConfigError::MissingHost);
                    };
                    config.host = value.parse().map_err(|_| ConfigError::InvalidHost {
                        value: value.clone(),
                    })?;
                }
                "--no-demo" => config.demo_seed = false,
                "--data" => {
                    if let Some(value) = args.next() {
                        config.data_dir = PathBuf::from(value);
                    }
                }
                "--help" | "-h" => return Ok(Parsed::Help),
                _ => {}
            }
        }

        Ok(Parsed::Run(config))
    }

    /// Parses the real process environment, exiting on help or parse failure.
    pub fn parse_env() -> Parsed {
        match Config::parse(std::env::args().skip(1)) {
            Ok(parsed) => parsed,
            Err(err) => {
                eprintln!("runecord: {err}");
                std::process::exit(1);
            }
        }
    }
}

/// The text printed by `--help`.
pub const HELP: &str =
    "runecord-server [--host 127.0.0.1] [--port 47800] [--data data] [--no-demo]";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_and_overrides() {
        let Parsed::Run(defaults) = Config::parse(Vec::<String>::new()).unwrap() else {
            panic!("expected run");
        };
        assert_eq!(defaults, Config::default());

        let Parsed::Run(config) = Config::parse([
            "--host",
            "0.0.0.0",
            "--port",
            "49000",
            "--data",
            "runtime",
            "--no-demo",
        ])
        .unwrap() else {
            panic!("expected run");
        };

        assert_eq!(config.host, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert_eq!(config.port, 49_000);
        assert_eq!(config.data_dir, PathBuf::from("runtime"));
        assert!(!config.demo_seed);
    }

    #[test]
    fn reports_help_and_bad_hosts() {
        assert_eq!(Config::parse(["--help"]).unwrap(), Parsed::Help);
        assert!(Config::parse(["--host"]).is_err());
        assert!(Config::parse(["--host", "not-an-ip"]).is_err());
    }
}
