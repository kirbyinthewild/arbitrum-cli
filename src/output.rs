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
            println!("{}", serde_json::to_string(value).unwrap_or_default());
        }
        Mode::Human => {
            println!("\n  {} {}", "✓".green().bold(), label.bold());
            println!("  {}", "─".repeat(50).dimmed());
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
                        println!("{}{}", pad, k.cyan().bold());
                        print_value(v, indent + 2);
                    }
                    _ => {
                        let val_str = match v {
                            Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        };
                        println!("{}{}: {}", pad, k.cyan(), val_str);
                    }
                }
            }
        }
        Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                println!("{}[{}]", pad, i.to_string().dimmed());
                print_value(item, indent + 2);
            }
        }
        other => {
            println!("{}{}", pad, other);
        }
    }
}
