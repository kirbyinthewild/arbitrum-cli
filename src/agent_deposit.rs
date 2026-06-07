//! `AgentDeposit` — Create Protocol agent funding primitive.
//!
//! `AgentDeposit` is the core contract in [Create Protocol] Phase 1: agents
//! register, deposit USDC, spend it on compute/tasks, and the protocol
//! distributes fees. This module wraps the read-only surface (balance + agent
//! metadata) so an AI agent can introspect its own on-chain state using only
//! this CLI.
//!
//! Write actions (`deposit`, `withdraw`) currently return unsigned calldata —
//! agents sign with their own key/switchboard and broadcast via `exec
//! eth_sendRawTransaction` (or any wallet). This keeps the CLI read-safe by
//! default; no keys ever touch this binary.
//!
//! **Contract addresses.** `AgentDeposit` is deployed on Sepolia today; Arbitrum
//! One redeployment lands with Phase 1. To wire the real address, update the
//! [`agent_deposit_address`] match — it's a one-line swap per chain.
//!
//! [Create Protocol]: https://createprotocol.org

use crate::rpc::{hex_to_u64, rpc_call};
use eyre::{eyre, Result};
use serde_json::{json, Value};

/// ABI selectors for the Create Protocol `AgentDeposit` contract.
///
/// These match the deployed Sepolia ABI (`AgentDeposit.sol`). Kept as string
/// constants so they're easy to eyeball against an ABI file or a block
/// explorer.
pub mod selectors {
    /// `balanceOf(address)` — USDC balance the agent has on deposit.
    pub const BALANCE_OF: &str = "0x70a08231";
    /// `deposit(uint256)` — pull USDC from caller into the agent's balance.
    pub const DEPOSIT: &str = "0xb6b55f25";
    /// `withdraw(uint256)` — push USDC back to caller.
    pub const WITHDRAW: &str = "0x2e1a7d4d";
    /// `isRegistered(address)` — whether the address is a known agent.
    pub const IS_REGISTERED: &str = "0xc3c5a547";
}

/// Resolve the `AgentDeposit` contract address for a given chain id.
///
/// Returns `None` until Create Protocol Phase 1 lands on that chain. Swapping
/// in a real deployment is one line per arm.
///
/// - Arbitrum One (42161): placeholder — Phase 1 deploy pending
/// - Arbitrum Sepolia (421614): placeholder — mirrors staging deploy
/// - All other chains: unsupported (`AgentDeposit` is Arbitrum-first)
pub fn agent_deposit_address(chain_id: u64) -> Option<&'static str> {
    match chain_id {
        // TODO(create-protocol): replace with real Arbitrum One/Sepolia deployment
        42_161 | 421_614 => Some("0x0000000000000000000000000000000000000000"),
        _ => None,
    }
}

/// What the user asked us to do with the `AgentDeposit` contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Read: `balanceOf(agent)` — returns USDC on deposit (raw + decimal).
    Balance,
    /// Prepare unsigned calldata for `deposit(amount)`.
    Deposit,
    /// Prepare unsigned calldata for `withdraw(amount)`.
    Withdraw,
    /// Read: `isRegistered(agent)` — has the agent joined Phase 1 registry?
    Registered,
}

impl Action {
    pub fn parse(s: &str) -> Result<Action> {
        match s.to_ascii_lowercase().as_str() {
            "balance" => Ok(Action::Balance),
            "deposit" => Ok(Action::Deposit),
            "withdraw" => Ok(Action::Withdraw),
            "registered" | "is-registered" => Ok(Action::Registered),
            other => Err(eyre!(
                "unknown agent-deposit action '{other}'. Try: balance | deposit | withdraw | registered"
            )),
        }
    }
}

/// Encode `selector(address)` calldata (one 32-byte word).
///
/// Pads the 20-byte address into a 32-byte left-zero-padded word. Accepts
/// `0x…` and bare hex. Validates length + hex.
pub fn encode_address_call(selector: &str, address: &str) -> Result<String> {
    let addr_hex = address.trim_start_matches("0x");
    if addr_hex.len() != 40 {
        let len = addr_hex.len();
        return Err(eyre!(
            "address must be 20 bytes (40 hex chars); got {len} chars"
        ));
    }
    hex::decode(addr_hex).map_err(|e| eyre!("address is not valid hex: {e}"))?;

    let sel = selector.trim_start_matches("0x");
    if sel.len() != 8 {
        let len = sel.len();
        return Err(eyre!(
            "selector must be 4 bytes (8 hex chars); got {len}"
        ));
    }
    hex::decode(sel).map_err(|e| eyre!("selector is not valid hex: {e}"))?;

    let addr_hex = addr_hex.to_lowercase();
    Ok(format!("0x{sel}{addr_hex:0>64}"))
}

/// Encode `selector(uint256)` calldata.
///
/// The amount is a raw on-chain integer (e.g., USDC has 6 decimals, so
/// $1.00 = `1_000_000`). We keep the CLI honest — no hidden decimal scaling.
pub fn encode_uint256_call(selector: &str, amount: u128) -> Result<String> {
    let sel = selector.trim_start_matches("0x");
    if sel.len() != 8 {
        let len = sel.len();
        return Err(eyre!(
            "selector must be 4 bytes (8 hex chars); got {len}"
        ));
    }
    hex::decode(sel).map_err(|e| eyre!("selector is not valid hex: {e}"))?;

    Ok(format!("0x{sel}{amount:0>64x}"))
}

/// Decode a 32-byte hex word as a u128 (fits USDC amounts up to ~3.4e20 raw).
pub fn decode_uint_result(hex_word: &str) -> Result<u128> {
    let stripped = hex_word.trim_start_matches("0x");
    if stripped.is_empty() {
        return Ok(0);
    }
    u128::from_str_radix(stripped, 16).map_err(|e| eyre!("invalid hex uint: {e}"))
}

/// Fetch the chain id from the RPC endpoint so we can resolve the right
/// `AgentDeposit` deployment without asking the user.
pub async fn fetch_chain_id(rpc: &str) -> Result<u64> {
    let result = rpc_call(rpc, "eth_chainId", json!([])).await?;
    let hex = result
        .as_str()
        .ok_or_else(|| eyre!("eth_chainId returned non-string"))?;
    hex_to_u64(hex)
}

/// Call `balanceOf(agent)` on `AgentDeposit`. Returns raw USDC (6 decimals).
pub async fn read_balance(rpc: &str, contract: &str, agent: &str) -> Result<u128> {
    let data = encode_address_call(selectors::BALANCE_OF, agent)?;
    let result = rpc_call(
        rpc,
        "eth_call",
        json!([{"to": contract, "data": data}, "latest"]),
    )
    .await?;
    let raw = result.as_str().unwrap_or("0x0");
    decode_uint_result(raw)
}

/// Call `isRegistered(agent)`. A non-zero return means the agent is in the
/// Phase 1 registry.
pub async fn read_registered(rpc: &str, contract: &str, agent: &str) -> Result<bool> {
    let data = encode_address_call(selectors::IS_REGISTERED, agent)?;
    let result = rpc_call(
        rpc,
        "eth_call",
        json!([{"to": contract, "data": data}, "latest"]),
    )
    .await?;
    let raw = result.as_str().unwrap_or("0x0");
    Ok(decode_uint_result(raw).unwrap_or(0) != 0)
}

/// Build the JSON response for the CLI `agent-deposit` command. Kept in this
/// module so the shape is guaranteed stable across commands.rs and tests.
pub fn format_balance_response(
    agent: &str,
    contract: &str,
    chain_id: u64,
    balance_raw: u128,
) -> Value {
    // AgentDeposit settles in USDC — 6 decimals. Scaling stays local to
    // presentation; the raw value is always the source of truth.
    const USDC_DECIMALS: u32 = 6;
    #[allow(clippy::cast_precision_loss)]
    let balance_human = balance_raw as f64 / 10f64.powi(i32::try_from(USDC_DECIMALS).unwrap_or(6));
    json!({
        "agent": agent,
        "contract": contract,
        "chain_id": chain_id,
        "action": "balance",
        "balance_raw": balance_raw.to_string(),
        "balance_usdc": balance_human,
    })
}

/// Build the JSON response for an unsigned write (deposit / withdraw).
pub fn format_unsigned_tx(
    action: &str,
    agent: &str,
    contract: &str,
    chain_id: u64,
    amount_raw: u128,
    data: &str,
) -> Value {
    json!({
        "agent": agent,
        "contract": contract,
        "chain_id": chain_id,
        "action": action,
        "amount_raw": amount_raw.to_string(),
        "tx": {
            "to": contract,
            "from": agent,
            "data": data,
            "value": "0x0",
        },
        "note": "Unsigned tx. Sign with your agent key (e.g., via kcolbchain switchboard) and broadcast via eth_sendRawTransaction.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_balance_of_call() {
        let data = encode_address_call(
            selectors::BALANCE_OF,
            "0xC75020d5f669F5D15Afcb81b0e5F6d21bCDa9664",
        )
        .expect("encode");
        assert_eq!(
            data,
            "0x70a08231000000000000000000000000c75020d5f669f5d15afcb81b0e5f6d21bcda9664"
        );
        assert_eq!(data.len(), 2 + 8 + 64);
    }

    #[test]
    fn encodes_is_registered_call() {
        let data = encode_address_call(
            selectors::IS_REGISTERED,
            "0x0000000000000000000000000000000000000001",
        )
        .expect("encode");
        assert!(data.starts_with("0xc3c5a547"));
        assert!(data.ends_with("0000000000000000000000000000000000000001"));
    }

    #[test]
    fn encodes_deposit_amount() {
        // 1 USDC = 1_000_000 (6 decimals)
        let data = encode_uint256_call(selectors::DEPOSIT, 1_000_000).expect("encode");
        assert_eq!(
            data,
            "0xb6b55f2500000000000000000000000000000000000000000000000000000000000f4240"
        );
    }

    #[test]
    fn encodes_withdraw_amount() {
        let data = encode_uint256_call(selectors::WITHDRAW, 42).expect("encode");
        assert_eq!(
            data,
            "0x2e1a7d4d000000000000000000000000000000000000000000000000000000000000002a"
        );
    }

    #[test]
    fn rejects_bad_address_length() {
        let err = encode_address_call(selectors::BALANCE_OF, "0xdeadbeef").unwrap_err();
        assert!(err.to_string().contains("20 bytes"));
    }

    #[test]
    fn rejects_bad_hex() {
        let err = encode_address_call(
            selectors::BALANCE_OF,
            "0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ",
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("hex"));
    }

    #[test]
    fn decodes_uint_result() {
        assert_eq!(decode_uint_result("0x0").unwrap(), 0);
        assert_eq!(decode_uint_result("0x").unwrap(), 0);
        assert_eq!(decode_uint_result("0xf4240").unwrap(), 1_000_000);
    }

    #[test]
    fn action_parses_aliases() {
        assert_eq!(Action::parse("balance").unwrap(), Action::Balance);
        assert_eq!(Action::parse("Deposit").unwrap(), Action::Deposit);
        assert_eq!(Action::parse("WITHDRAW").unwrap(), Action::Withdraw);
        assert_eq!(Action::parse("registered").unwrap(), Action::Registered);
        assert_eq!(Action::parse("is-registered").unwrap(), Action::Registered);
        assert!(Action::parse("yeet").is_err());
    }

    #[test]
    fn known_chains_resolve_address() {
        // Both arms return Some — the value is placeholder until Phase 1,
        // but the resolver contract must remain stable.
        assert!(agent_deposit_address(42_161).is_some());
        assert!(agent_deposit_address(421_614).is_some());
        assert!(agent_deposit_address(1).is_none());
    }

    #[test]
    fn balance_response_scales_usdc_decimals() {
        let v = format_balance_response(
            "0xC75020d5f669F5D15Afcb81b0e5F6d21bCDa9664",
            "0x0000000000000000000000000000000000000000",
            42161,
            2_500_000, // 2.5 USDC raw
        );
        assert_eq!(v["balance_raw"], "2500000");
        assert_eq!(v["balance_usdc"], 2.5);
        assert_eq!(v["action"], "balance");
    }

    #[test]
    fn unsigned_tx_response_has_expected_shape() {
        let v = format_unsigned_tx(
            "deposit",
            "0xC75020d5f669F5D15Afcb81b0e5F6d21bCDa9664",
            "0x0000000000000000000000000000000000000000",
            42161,
            1_000_000,
            "0xb6b55f2500000000000000000000000000000000000000000000000000000000000f4240",
        );
        assert_eq!(v["tx"]["value"], "0x0");
        assert_eq!(v["tx"]["to"], "0x0000000000000000000000000000000000000000");
        assert_eq!(v["amount_raw"], "1000000");
        assert!(v["note"].as_str().unwrap().contains("Unsigned"));
    }
}
