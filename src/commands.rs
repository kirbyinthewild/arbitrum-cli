use crate::agent_deposit::{
    agent_deposit_address, encode_uint256_call, fetch_chain_id, format_balance_response,
    format_unsigned_tx, read_balance, read_registered, selectors, Action,
};
use crate::output::{emit, Mode};
use crate::rpc::{hex_to_u64, rpc_call, wei_hex_to_eth};
use clap::Command;
use colored::Colorize;
use eyre::{eyre, Result};
use serde_json::{json, Value};

struct SupportedChain {
    chain_id: u64,
    name: &'static str,
    rpc_default: &'static str,
    explorer: &'static str,
    usdc: &'static str,
    uniswap_v3_quoter: Option<&'static str>,
}

const SUPPORTED_CHAINS: &[SupportedChain] = &[
    SupportedChain {
        chain_id: 42_161,
        name: "Arbitrum One",
        rpc_default: "https://arb1.arbitrum.io/rpc",
        explorer: "https://arbiscan.io",
        usdc: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
        uniswap_v3_quoter: Some("0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6"),
    },
    SupportedChain {
        chain_id: 421_614,
        name: "Arbitrum Sepolia",
        rpc_default: "https://sepolia-rollup.arbitrum.io/rpc",
        explorer: "https://sepolia.arbiscan.io",
        usdc: "0x75faf114eafb1BDbe2F0316DF893fd58CE46AA4d",
        uniswap_v3_quoter: None,
    },
];

// ── block ──
pub async fn block(rpc: &str, block: &str, mode: Mode) -> Result<()> {
    let block_param = if block == "latest" || block == "earliest" || block == "pending" {
        json!(block)
    } else {
        let n: u64 = block.parse().map_err(|_| eyre!("invalid block number"))?;
        json!(format!("0x{n:x}"))
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
        return Err(eyre!("Transaction not found: {hash}"));
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
    let address = address.trim_start_matches("0x");
    let padded = format!("{address:0>64}");
    let data = format!("0x70a08231{padded}");
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
    #[allow(clippy::cast_precision_loss)]
    let balance_human = balance_raw as f64 / 10f64.powi(i32::try_from(decimals).unwrap_or(18));

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
    #[allow(clippy::cast_precision_loss)]
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
            "Unsupported watch target: {target}. Use 'blocks' for now."
        ))
    }
}

// ── exec (generic RPC passthrough — agent-friendly) ──
pub async fn exec(rpc: &str, method: &str, params: &str, mode: Mode) -> Result<()> {
    let params_val: Value =
        serde_json::from_str(params).map_err(|e| eyre!("Invalid params JSON: {e}"))?;
    let result = rpc_call(rpc, method, params_val).await?;
    let out = json!({
        "method": method,
        "result": result,
    });
    emit(mode, "Exec", &out);
    Ok(())
}

fn chain_inventory() -> Vec<Value> {
    SUPPORTED_CHAINS
        .iter()
        .map(|chain| {
            json!({
                "chain_id": chain.chain_id,
                "name": chain.name,
                "rpc_default": chain.rpc_default,
                "explorer": chain.explorer,
                "contracts": {
                    "usdc": chain.usdc,
                    "agent_deposit": agent_deposit_address(chain.chain_id),
                    "uniswap_v3_quoter": chain.uniswap_v3_quoter,
                },
            })
        })
        .collect()
}

fn subcommand_inventory(command: &Command) -> Vec<Value> {
    command
        .get_subcommands()
        .map(|subcommand| {
            let args: Vec<Value> = subcommand
                .get_arguments()
                .map(|arg| {
                    json!({
                        "name": arg.get_id().as_str(),
                        "required": arg.is_required_set(),
                        "help": arg.get_help().map(std::string::ToString::to_string),
                    })
                })
                .collect();

            json!({
                "name": subcommand.get_name(),
                "description": subcommand.get_about().map(std::string::ToString::to_string),
                "args": args,
            })
        })
        .collect()
}

pub(crate) fn info_inventory(command: &Command) -> Value {
    json!({
        "name": "arbitrum-cli",
        "version": env!("CARGO_PKG_VERSION"),
        "brand": "kcolbchain",
        "chains": chain_inventory(),
        "subcommands": subcommand_inventory(command),
    })
}

fn print_info_human(inventory: &Value) {
    let check = "✓".green().bold();
    let title = "arbitrum-cli info".bold();
    println!("\n  {check} {title}");
    let divider = "─".repeat(72).dimmed();
    println!("  {divider}");
    let version = inventory["version"].as_str().unwrap_or_default();
    let version_label = "version:".cyan();
    println!("  {version_label} {version}");

    let chains = "Chains".cyan().bold();
    println!("\n  {chains}");
    println!(
        "  {:<18} {:<8} RPC                                    Explorer",
        "Name", "Chain"
    );
    for chain in inventory["chains"].as_array().into_iter().flatten() {
        let name = chain["name"].as_str().unwrap_or_default();
        let chain_id = chain["chain_id"].as_u64().unwrap_or_default();
        let rpc_default = chain["rpc_default"].as_str().unwrap_or_default();
        let explorer = chain["explorer"].as_str().unwrap_or_default();
        println!("  {name:<18} {chain_id:<8} {rpc_default:<38} {explorer}");
        let contracts = &chain["contracts"];
        let usdc = contracts["usdc"].as_str().unwrap_or("not configured");
        println!("    USDC: {usdc}");
        let agent_deposit = contracts["agent_deposit"]
            .as_str()
            .unwrap_or("not configured");
        println!("    AgentDeposit: {agent_deposit}");
        let uniswap_v3_quoter = contracts["uniswap_v3_quoter"]
            .as_str()
            .unwrap_or("not configured");
        println!("    Uniswap V3 Quoter: {uniswap_v3_quoter}");
    }

    let subcommands = "Subcommands".cyan().bold();
    println!("\n  {subcommands}");
    println!("  Name             Description");
    for subcommand in inventory["subcommands"].as_array().into_iter().flatten() {
        let name = subcommand["name"].as_str().unwrap_or_default();
        let description = subcommand["description"].as_str().unwrap_or_default();
        println!("  {name:<16} {description}");
    }
    println!();
}

// ── info ──
#[allow(clippy::unnecessary_wraps)]
pub fn info(mode: Mode, command: &Command) -> Result<()> {
    let inventory = info_inventory(command);
    match mode {
        Mode::Json => emit(mode, "arbitrum-cli info", &inventory),
        Mode::Human => print_info_human(&inventory),
    }
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
                    "No AgentDeposit deployment registered for chain id {chain_id}. \
                     Pass --contract <address> or use an Arbitrum RPC.",
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
    Ok(())
}

// ── mcp (stub) ──
#[allow(clippy::unnecessary_wraps)]
pub fn mcp(_rpc: &str, bind: &str) -> Result<()> {
    // MCP server stub — production version would expose tools via stdio or SSE
    // following the Model Context Protocol spec.
    eprintln!("MCP server mode — stub implementation");
    eprintln!("Bind: {bind}");
    eprintln!("Tools exposed: block, tx, balance, token, call, gas, exec");
    eprintln!();
    eprintln!("Full MCP implementation coming — this stub validates the tool shape.");
    eprintln!("See: https://modelcontextprotocol.io");
    Ok(())
}
