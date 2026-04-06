use eyre::{eyre, Result};
use serde_json::{json, Value};

/// Minimal JSON-RPC client for Arbitrum (or any EVM chain).
pub async fn rpc_call(url: &str, method: &str, params: Value) -> Result<Value> {
    let client = reqwest::Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    });

    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await?
        .json::<Value>()
        .await?;

    if let Some(err) = resp.get("error") {
        return Err(eyre!("RPC error: {}", err));
    }

    resp.get("result")
        .cloned()
        .ok_or_else(|| eyre!("No result field in RPC response"))
}

/// Convert a hex string (0x-prefixed) to a u64.
pub fn hex_to_u64(hex: &str) -> Result<u64> {
    let stripped = hex.trim_start_matches("0x");
    u64::from_str_radix(stripped, 16).map_err(|e| eyre!("Invalid hex: {}", e))
}

/// Convert wei (as hex string) to ETH as f64.
pub fn wei_hex_to_eth(hex: &str) -> Result<f64> {
    let stripped = hex.trim_start_matches("0x");
    let wei = u128::from_str_radix(stripped, 16).map_err(|e| eyre!("Invalid wei: {}", e))?;
    Ok(wei as f64 / 1e18)
}
