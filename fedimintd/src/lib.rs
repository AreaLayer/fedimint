#![deny(clippy::pedantic)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::large_futures)]

/// Module for creating `fedimintd` binary with custom modules
use bitcoin::Network;
use fedimint_core::envs::BitcoinRpcConfig;
use fedimint_core::util::SafeUrl;
pub use fedimintd::*;

mod fedimintd;

pub mod envs;
use crate::envs::{FM_DEFAULT_ESPLORA_API_ENV, FM_PORT_ESPLORA_ENV};

#[allow(clippy::map_unwrap_or)]
pub fn default_esplora_server(network: Network) -> BitcoinRpcConfig {
    let url = std::env::var(FM_DEFAULT_ESPLORA_API_ENV)
        .ok()
        .map(|s| SafeUrl::parse(&s).expect("Failed to parse default esplora server"))
        .unwrap_or_else(|| match network {
            Network::Bitcoin => SafeUrl::parse("https://blockstream.info/api/")
                .expect("Failed to parse default esplora server"),
            Network::Testnet => SafeUrl::parse("https://blockstream.info/testnet/api/")
                .expect("Failed to parse default esplora server"),
            Network::Regtest => SafeUrl::parse(&format!(
                "http://127.0.0.1:{}/",
                std::env::var(FM_PORT_ESPLORA_ENV).unwrap_or(String::from("50002"))
            ))
            .expect("Failed to parse default esplora server"),
            Network::Signet => SafeUrl::parse("https://blockstream.info/signet/api/")
                .expect("Failed to parse default esplora server"),
            _ => panic!("Failed to parse default esplora server"),
        });
    BitcoinRpcConfig {
        kind: "esplora".to_string(),
        url,
    }
}
