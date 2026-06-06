use colored::Colorize;
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Json,
    Human,
}

/// Emit a result to stdout in the requested mode.
/// JSON mode: raw JSON for agents (default).
/// Human mode: pretty colored output for terminals.
pub fn emit(mode: Mode, label: &str, value: &Value) {
    match mode {
        Mode::Json => {
            let value = serde_json::to_string(value).unwrap_or_default();
            println!("{value}");
        }
        Mode::Human => {
            let check = "✓".green().bold();
            let label = label.bold();
            println!("\n  {check} {label}");
            let divider = "─".repeat(50).dimmed();
            println!("  {divider}");
            print_value(value, 2);
            println!();
        }
    }
}

fn print_value(value: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::Object(_) | Value::Array(_) => {
                        let key = k.cyan().bold();
                        println!("{pad}{key}");
                        print_value(v, indent + 2);
                    }
                    _ => {
                        let val_str = match v {
                            Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        };
                        let key = k.cyan();
                        println!("{pad}{key}: {val_str}");
                    }
                }
            }
        }
        Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                let index = i.to_string().dimmed();
                println!("{pad}[{index}]");
                print_value(item, indent + 2);
            }
        }
        other => {
            println!("{pad}{other}");
        }
    }
}
