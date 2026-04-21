use clap::{Parser, Subcommand};

mod agent_deposit;
mod commands;
mod output;
mod rpc;

#[derive(Parser)]
#[command(
    name = "arbitrum-cli",
    version,
    about = "Agent-first Arbitrum CLI — JSON in, JSON out, MCP-compatible",
    long_about = "A single Rust binary to query Arbitrum, interact with contracts, monitor events, and expose an MCP server for AI agents. Default output is JSON (agent-friendly). Use --human for pretty terminal output."
)]
struct Cli {
    /// RPC URL (default: https://arb1.arbitrum.io/rpc)
    #[arg(long, global = true, env = "ARBITRUM_RPC_URL")]
    rpc: Option<String>,

    /// Pretty-print output for humans instead of raw JSON
    #[arg(long, global = true)]
    human: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Query block info by number or "latest"
    Block {
        /// Block number (or "latest", "earliest", "pending")
        block: String,
    },

    /// Query transaction by hash
    Tx {
        /// Transaction hash
        hash: String,
    },

    /// Get native ETH balance for an address
    Balance {
        /// Wallet address
        address: String,
    },

    /// Query ERC-20 token balance
    Token {
        /// Token contract address
        token: String,

        /// Wallet address to query
        address: String,
    },

    /// Read from a contract (eth_call)
    Call {
        /// Contract address
        to: String,

        /// Calldata (hex-encoded)
        #[arg(long)]
        data: String,
    },

    /// Get current gas price info
    Gas,

    /// Stream new blocks (polling)
    Watch {
        /// What to watch: blocks, pending, or an address
        target: String,
    },

    /// Execute a generic JSON-RPC call (agent-friendly)
    Exec {
        /// RPC method name (e.g., eth_blockNumber)
        method: String,

        /// Params as JSON array
        #[arg(long, default_value = "[]")]
        params: String,
    },

    /// Start an MCP server exposing arbitrum-cli as tools for AI agents
    Mcp {
        /// Bind address
        #[arg(long, default_value = "127.0.0.1:3456")]
        bind: String,
    },

    /// Interact with Create Protocol AgentDeposit on Arbitrum
    ///
    /// Read agent balance / registration state, or produce unsigned calldata
    /// for deposit/withdraw (sign + broadcast externally — the CLI never
    /// touches keys).
    AgentDeposit {
        /// Agent wallet address (the EOA that registers + deposits)
        address: String,

        /// What to do: balance | deposit | withdraw | registered
        #[arg(long, default_value = "balance")]
        action: String,

        /// Amount in raw on-chain units (USDC = 6 decimals; 1 USDC = 1000000).
        /// Required for deposit / withdraw.
        #[arg(long)]
        amount: Option<u128>,

        /// Override the AgentDeposit contract address (advanced; defaults to
        /// the deployment registered for the connected chain).
        #[arg(long)]
        contract: Option<String>,
    },

    /// Print supported chains and info
    Info,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cli = Cli::parse();
    let rpc_url = cli
        .rpc
        .unwrap_or_else(|| "https://arb1.arbitrum.io/rpc".to_string());
    let out_mode = if cli.human {
        output::Mode::Human
    } else {
        output::Mode::Json
    };

    match cli.command {
        Commands::Block { block } => commands::block(&rpc_url, &block, out_mode).await?,
        Commands::Tx { hash } => commands::tx(&rpc_url, &hash, out_mode).await?,
        Commands::Balance { address } => commands::balance(&rpc_url, &address, out_mode).await?,
        Commands::Token { token, address } => {
            commands::token_balance(&rpc_url, &token, &address, out_mode).await?
        }
        Commands::Call { to, data } => commands::call(&rpc_url, &to, &data, out_mode).await?,
        Commands::Gas => commands::gas(&rpc_url, out_mode).await?,
        Commands::Watch { target } => commands::watch(&rpc_url, &target, out_mode).await?,
        Commands::Exec { method, params } => {
            commands::exec(&rpc_url, &method, &params, out_mode).await?
        }
        Commands::Mcp { bind } => commands::mcp(&rpc_url, &bind).await?,
        Commands::AgentDeposit {
            address,
            action,
            amount,
            contract,
        } => {
            commands::agent_deposit(
                &rpc_url,
                &address,
                &action,
                amount,
                contract.as_deref(),
                out_mode,
            )
            .await?
        }
        Commands::Info => commands::info(out_mode)?,
    }

    Ok(())
}
