/// Configuration for the Nord client.
#[derive(Debug, Clone)]
pub struct NordConfig {
    /// Base URL for the Nord web server (e.g. `https://zo-mainnet.n1.xyz`).
    pub web_server_url: String,
    /// App address on Solana.
    pub app: String,
    /// Solana RPC URL.
    pub solana_rpc_url: String,
    /// Proton URL; defaults to `web_server_url` if not set.
    pub proton_url: Option<String>,
}
