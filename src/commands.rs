use crate::output::{emit, Mode};
use crate::rpc::{hex_to_u64, rpc_call, wei_hex_to_eth};
use eyre::{eyre, Result};
use serde_json::{json, Value};

// ── block ──
pub async fn block(rpc: &str, block: &str, mode: Mode) -> Result<()> {
    let block_param = if block == "latest" || block == "earliest" || block == "pending" {
        json!(block)
    } else {
        let n: u64 = block.parse().map_err(|_| eyre!("invalid block number"))?;
        json!(format!("0x{:x}", n))
    };
    let result = rpc_call(rpc, "eth_getBlockByNumber", json!([block_param, false])).await?;

    // Annotate with decoded fields for humans
    let mut annotated = result.clone();
    if let Some(obj) = annotated.as_object_mut() {
        if let Some(num) = obj.get("number").and_then(|v| v.as_str()) {
            if let Ok(n) = hex_to_u64(num) {
                obj.insert("number_decimal".to_string(), json!(n));
            }
        }
        if let Some(ts) = obj.get("timestamp").and_then(|v| v.as_str()) {
            if let Ok(t) = hex_to_u64(ts) {
                obj.insert("timestamp_decimal".to_string(), json!(t));
            }
        }
    }

    emit(mode, "Block", &annotated);
    Ok(())
}

// ── tx ──
pub async fn tx(rpc: &str, hash: &str, mode: Mode) -> Result<()> {
    let result = rpc_call(rpc, "eth_getTransactionByHash", json!([hash])).await?;
    if result.is_null() {
        return Err(eyre!("Transaction not found: {}", hash));
    }
    emit(mode, "Transaction", &result);
    Ok(())
}

// ── balance ──
pub async fn balance(rpc: &str, address: &str, mode: Mode) -> Result<()> {
    let result = rpc_call(rpc, "eth_getBalance", json!([address, "latest"])).await?;
    let wei_hex = result.as_str().unwrap_or("0x0");
    let eth = wei_hex_to_eth(wei_hex)?;
    let out = json!({
        "address": address,
        "balance_wei": wei_hex,
        "balance_eth": eth,
    });
    emit(mode, "Balance", &out);
    Ok(())
}

// ── token balance ──
pub async fn token_balance(rpc: &str, token: &str, address: &str, mode: Mode) -> Result<()> {
    // balanceOf(address) selector = 0x70a08231
    let padded = format!("{:0>64}", address.trim_start_matches("0x"));
    let data = format!("0x70a08231{}", padded);
    let result = rpc_call(
        rpc,
        "eth_call",
        json!([{"to": token, "data": data}, "latest"]),
    )
    .await?;
    let raw = result.as_str().unwrap_or("0x0");

    // Also fetch decimals
    let decimals_result = rpc_call(
        rpc,
        "eth_call",
        json!([{"to": token, "data": "0x313ce567"}, "latest"]),
    )
    .await
    .ok();
    let decimals = decimals_result
        .as_ref()
        .and_then(|v| v.as_str())
        .and_then(|s| hex_to_u64(s).ok())
        .unwrap_or(18);

    let balance_raw = u128::from_str_radix(raw.trim_start_matches("0x"), 16).unwrap_or(0);
    let balance_human = balance_raw as f64 / 10f64.powi(decimals as i32);

    let out = json!({
        "token": token,
        "address": address,
        "decimals": decimals,
        "balance_raw": raw,
        "balance": balance_human,
    });
    emit(mode, "Token Balance", &out);
    Ok(())
}

// ── call ──
pub async fn call(rpc: &str, to: &str, data: &str, mode: Mode) -> Result<()> {
    let result = rpc_call(rpc, "eth_call", json!([{"to": to, "data": data}, "latest"])).await?;
    let out = json!({
        "to": to,
        "data": data,
        "result": result,
    });
    emit(mode, "Contract Call", &out);
    Ok(())
}

// ── gas ──
pub async fn gas(rpc: &str, mode: Mode) -> Result<()> {
    let gas_price = rpc_call(rpc, "eth_gasPrice", json!([])).await?;
    let block_num = rpc_call(rpc, "eth_blockNumber", json!([])).await?;

    let gas_hex = gas_price.as_str().unwrap_or("0x0");
    let gwei = u128::from_str_radix(gas_hex.trim_start_matches("0x"), 16).unwrap_or(0) as f64 / 1e9;

    let out = json!({
        "gas_price_wei": gas_hex,
        "gas_price_gwei": gwei,
        "block_number": block_num,
    });
    emit(mode, "Gas", &out);
    Ok(())
}

// ── watch ──
pub async fn watch(rpc: &str, target: &str, mode: Mode) -> Result<()> {
    use std::time::Duration;
    use tokio::time::sleep;

    if target == "blocks" {
        let mut last = 0u64;
        loop {
            let result = rpc_call(rpc, "eth_blockNumber", json!([])).await?;
            if let Some(hex) = result.as_str() {
                let n = hex_to_u64(hex).unwrap_or(0);
                if n != last {
                    let out = json!({ "block": n, "hex": hex });
                    emit(mode, "New Block", &out);
                    last = n;
                }
            }
            sleep(Duration::from_secs(2)).await;
        }
    } else {
        Err(eyre!(
            "Unsupported watch target: {}. Use 'blocks' for now.",
            target
        ))
    }
}

// ── exec (generic RPC passthrough — agent-friendly) ──
pub async fn exec(rpc: &str, method: &str, params: &str, mode: Mode) -> Result<()> {
    let params_val: Value = serde_json::from_str(params)
        .map_err(|e| eyre!("Invalid params JSON: {}", e))?;
    let result = rpc_call(rpc, method, params_val).await?;
    let out = json!({
        "method": method,
        "result": result,
    });
    emit(mode, "Exec", &out);
    Ok(())
}

// ── info ──
pub fn info(mode: Mode) -> Result<()> {
    let out = json!({
        "name": "arbitrum-cli",
        "version": env!("CARGO_PKG_VERSION"),
        "brand": "kcolbchain",
        "chains": {
            "arbitrum_one": {
                "chain_id": 42161,
                "rpc": "https://arb1.arbitrum.io/rpc",
                "explorer": "https://arbiscan.io"
            },
            "arbitrum_nova": {
                "chain_id": 42170,
                "rpc": "https://nova.arbitrum.io/rpc",
                "explorer": "https://nova.arbiscan.io"
            },
            "arbitrum_sepolia": {
                "chain_id": 421614,
                "rpc": "https://sepolia-rollup.arbitrum.io/rpc",
                "explorer": "https://sepolia.arbiscan.io"
            }
        }
    });
    emit(mode, "arbitrum-cli info", &out);
    Ok(())
}

// ── mcp (stub) ──
pub async fn mcp(_rpc: &str, bind: &str) -> Result<()> {
    // MCP server stub — production version would expose tools via stdio or SSE
    // following the Model Context Protocol spec.
    eprintln!("MCP server mode — stub implementation");
    eprintln!("Bind: {}", bind);
    eprintln!("Tools exposed: block, tx, balance, token, call, gas, exec");
    eprintln!();
    eprintln!("Full MCP implementation coming — this stub validates the tool shape.");
    eprintln!("See: https://modelcontextprotocol.io");
    Ok(())
}
