use crate::agent_deposit::{
    self, agent_deposit_address, encode_address_call, encode_uint256_call, fetch_chain_id,
    format_balance_response, format_unsigned_tx, read_balance, read_registered, selectors, Action,
};
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
    let params_val: Value =
        serde_json::from_str(params).map_err(|e| eyre!("Invalid params JSON: {}", e))?;
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

// ── agent-deposit (Create Protocol) ──
//
// Phase 1 of Create Protocol is an agent-economy on Arbitrum: agents register,
// deposit USDC, execute tasks, earn fees. This command gives the CLI a
// first-class verb for the AgentDeposit contract so an LLM can do the full
// loop (check balance → prepare deposit/withdraw tx) from one binary.
//
// Reads go through `eth_call` and return decoded JSON. Writes return unsigned
// calldata — the CLI is strictly key-less; agents sign with switchboard or
// any wallet and broadcast via `exec eth_sendRawTransaction`.
pub async fn agent_deposit(
    rpc: &str,
    address: &str,
    action_str: &str,
    amount: Option<u128>,
    contract_override: Option<&str>,
    mode: Mode,
) -> Result<()> {
    let action = Action::parse(action_str)?;

    // Resolve contract: explicit override wins, else look up by chain id. We
    // call eth_chainId either way so the emitted JSON records which chain
    // the caller is actually talking to — avoids silent testnet/mainnet
    // confusion when an agent is driving.
    let chain_id = fetch_chain_id(rpc).await?;
    let contract = match contract_override {
        Some(c) => c.to_string(),
        None => agent_deposit_address(chain_id)
            .ok_or_else(|| {
                eyre!(
                    "No AgentDeposit deployment registered for chain id {}. \
                     Pass --contract <address> or use an Arbitrum RPC.",
                    chain_id
                )
            })?
            .to_string(),
    };

    match action {
        Action::Balance => {
            let raw = read_balance(rpc, &contract, address).await?;
            let out = format_balance_response(address, &contract, chain_id, raw);
            emit(mode, "Agent Deposit Balance", &out);
        }
        Action::Registered => {
            let is_reg = read_registered(rpc, &contract, address).await?;
            let out = json!({
                "agent": address,
                "contract": contract,
                "chain_id": chain_id,
                "action": "registered",
                "registered": is_reg,
            });
            emit(mode, "Agent Registration", &out);
        }
        Action::Deposit | Action::Withdraw => {
            let amt = amount.ok_or_else(|| {
                eyre!("--amount is required for deposit/withdraw (raw units; USDC = 6 decimals)")
            })?;
            let (selector, label, action_tag) = match action {
                Action::Deposit => (selectors::DEPOSIT, "Agent Deposit (unsigned)", "deposit"),
                Action::Withdraw => (selectors::WITHDRAW, "Agent Withdraw (unsigned)", "withdraw"),
                _ => unreachable!(),
            };
            let data = encode_uint256_call(selector, amt)?;
            let out = format_unsigned_tx(action_tag, address, &contract, chain_id, amt, &data);
            emit(mode, label, &out);
        }
    }

    // Silence "unused import" if the module-private helpers change shape in
    // the future — these re-exports document the full surface used here.
    let _ = (agent_deposit::selectors::BALANCE_OF, encode_address_call);

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
