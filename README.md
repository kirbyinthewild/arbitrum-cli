# arbitrum-cli

### Agent-first Arbitrum CLI

*JSON in, JSON out. MCP-compatible. Single Rust binary. Built for LLMs, automation pipelines, and developers who think in JSON.*

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE)
[![kcolbchain](https://img.shields.io/badge/by-kcolbchain-gold?style=for-the-badge)](https://kcolbchain.com)

---

**[Quick Start](#quick-start)** · **[Commands](#commands)** · **[Agent mode](#agent-mode)** · **[Create Protocol](#create-protocol-agent-deposit)** · **[MCP](#mcp-server)**

---

## Why

Every blockchain CLI today is either a heavyweight toolkit (Foundry) or a chain-specific SDK bundle. Neither is built for agents. `arbitrum-cli` wraps Arbitrum RPC behind a clean surface that:

- Emits **JSON by default** so agents can pipe output anywhere
- Has a generic `exec` command for any RPC method
- Exposes tools via **MCP** so Claude, Cursor, or any AI agent can call it natively
- Ships as a **single Rust binary** — `cargo install arbitrum-cli` and go

Part of the agent-first blockchain CLI suite by [kcolbchain](https://kcolbchain.com).

## Quick start

```bash
cargo install arbitrum-cli

# Query the latest block (JSON output, agent-ready)
arbitrum-cli block latest

# Check an address balance
arbitrum-cli balance 0xC75020d5f669F5D15Afcb81b0e5F6d21bCDa9664

# Check a token balance (USDC on Arbitrum)
arbitrum-cli token \
  0xaf88d065e77c8cC2239327C5EDb3A432268e5831 \
  0xC75020d5f669F5D15Afcb81b0e5F6d21bCDa9664

# Get current gas price
arbitrum-cli gas

# Human-readable output
arbitrum-cli block latest --human
```

## Commands

| Command | What it does |
|---|---|
| `block <number\|latest>` | Fetch block info |
| `tx <hash>` | Fetch transaction by hash |
| `balance <address>` | Native ETH balance |
| `token <token> <address>` | ERC-20 balance with decimals |
| `call --data <hex> <to>` | Read contract via `eth_call` |
| `gas` | Current gas price + block number |
| `watch blocks` | Stream new blocks (polling) |
| `exec <method> --params '[...]'` | Generic RPC passthrough |
| `agent-deposit <address> --action ...` | Create Protocol AgentDeposit — balance, deposit, withdraw, registered |
| `mcp` | Start MCP server for AI agents |
| `info` | List supported Arbitrum chains |

## Agent mode

All commands default to JSON output. No flags, no ceremony — pipe it into `jq`, feed it to an LLM, or forward to any tool.

```bash
# Agent-friendly
arbitrum-cli balance 0xC750... | jq .balance_eth

# Generic RPC for agents — any method, any params
arbitrum-cli exec eth_chainId --params '[]'
arbitrum-cli exec eth_getLogs --params '[{"fromBlock":"latest","address":"0x..."}]'
```

Use `--human` for colored terminal output when you're debugging.

## Create Protocol agent-deposit

Works out of the box with [Create Protocol](https://createprotocol.org) AgentDeposit on Arbitrum. Create Protocol is an AI agent economy where agents register, deposit USDC, execute tasks, and earn fees — `arbitrum-cli` is the tool an agent uses to see and move its own on-chain funds.

```bash
# How much USDC does this agent have on deposit?
arbitrum-cli agent-deposit 0xC75020d5f669F5D15Afcb81b0e5F6d21bCDa9664 --action balance

# Is this agent registered in Phase 1?
arbitrum-cli agent-deposit 0xC75020d5f669F5D15Afcb81b0e5F6d21bCDa9664 --action registered

# Prepare an unsigned deposit tx (1 USDC = 1_000_000 raw, 6 decimals).
# The CLI never holds keys — sign + broadcast externally.
arbitrum-cli agent-deposit 0xC750... --action deposit --amount 1000000

# Same shape for withdraw
arbitrum-cli agent-deposit 0xC750... --action withdraw --amount 500000
```

The AgentDeposit contract address is resolved automatically per chain (Arbitrum One, Arbitrum Sepolia). Override with `--contract 0x…` for forks or staging deployments.

Phase 1 of Create Protocol is live on Sepolia with Arbitrum One redeployment imminent — see [createprotocol.org](https://createprotocol.org).

## MCP server

Expose arbitrum-cli as a [Model Context Protocol](https://modelcontextprotocol.io) server so Claude, Cursor, or any MCP-compatible agent can call Arbitrum directly.

```bash
arbitrum-cli mcp --bind 127.0.0.1:3456
```

Tools exposed:

- `arbitrum.block` · `arbitrum.tx` · `arbitrum.balance` · `arbitrum.token`
- `arbitrum.call` · `arbitrum.gas` · `arbitrum.exec`

*(MCP integration is stubbed in v0.1 — full stdio + SSE support in v0.2.)*

## Configuration

```bash
# Use a custom RPC (env var)
export ARBITRUM_RPC_URL=https://arb-mainnet.g.alchemy.com/v2/YOUR_KEY
arbitrum-cli block latest

# Or pass inline
arbitrum-cli --rpc https://arb1.arbitrum.io/rpc gas
```

Default RPC: `https://arb1.arbitrum.io/rpc` (Arbitrum One)

## Part of the kcolbchain agent-first suite

| Tool | Purpose |
|---|---|
| [`arbitrum-cli`](https://github.com/kcolbchain/arbitrum-cli) | Arbitrum chain access |
| [`superchain-trace`](https://github.com/kcolbchain/superchain-trace) | OP Superchain cross-chain message debugger |
| [`stylus-profiler`](https://github.com/kcolbchain/stylus-profiler) | Arbitrum Stylus WASM binary analyzer |
| [`gas-oracle`](https://github.com/kcolbchain/gas-oracle) | L2 gas cost prediction via blob fees |

## License

MIT — do whatever you want. Attribution appreciated.

## Contributing

See [kcolbchain.com/join](https://kcolbchain.com/join). Good first issues welcome.
