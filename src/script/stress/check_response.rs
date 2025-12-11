// Sequential URL checker for AdvCache endpoints.
//
// This tool generates realistic GETs to four endpoints:
//  1) /api/v1/user        (50%)
//  2) /api/v1/client      (25%)
//  3) /api/v1/buyer       (12.5%)
//  4) /api/v1/customer    (12.5%)
//
// For each response we:
//  - parse JSON structurally: data.attributes.title / data.attributes.description
//  - assert title starts with an integer (leading digits are extracted)
//  - assert description HAS path prefix (exact URL path+query we sent), allowing extra prose afterwards

use clap::Parser;
use reqwest::Client;
use serde_json::Value;
use std::io::Read;
use std::time::{Duration, Instant};
use brotli::Decompressor;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server host
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Server port
    #[arg(long, default_value_t = 8020)]
    port: u16,

    /// Start i (inclusive)
    #[arg(long, default_value_t = 1)]
    start: i32,

    /// Max i (inclusive)
    #[arg(long, default_value_t = 1000000)]
    max: i32,

    /// Test duration in seconds
    #[arg(long, default_value_t = 3600)]
    duration: u64,

    /// Per-request timeout in seconds
    #[arg(long, default_value_t = 5)]
    timeout: u64,

    /// User-Agent
    #[arg(long, default_value = "mixed-checker/1.0")]
    ua: String,

    /// Snippet length for debug
    #[arg(long, default_value_t = 64)]
    snip: usize,

    /// Force a single endpoint: user|client|buyer|customer (empty = mix)
    #[arg(long, default_value = "")]
    only: String,
}

#[derive(Debug, Clone, Copy)]
enum Endpoint {
    User,      // 50%
    Client,    // 25%
    Buyer,     // 12.5%
    Customer,  // 12.5%
}

impl Endpoint {
    fn path(&self) -> &'static str {
        match self {
            Endpoint::User => "/api/v1/user",
            Endpoint::Client => "/api/v1/client",
            Endpoint::Buyer => "/api/v1/buyer",
            Endpoint::Customer => "/api/v1/customer",
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Endpoint::User => "user",
            Endpoint::Client => "client",
            Endpoint::Buyer => "buyer",
            Endpoint::Customer => "customer",
        }
    }
}

fn choose_endpoint(mix: bool, forced: &str) -> Endpoint {
    if !mix {
        match forced.trim().to_lowercase().as_str() {
            "user" => return Endpoint::User,
            "client" => return Endpoint::Client,
            "buyer" => return Endpoint::Buyer,
            "customer" => return Endpoint::Customer,
            _ => eprintln!("unknown --only={:?}, falling back to mix", forced),
        }
    }
    // Mix by weighted dice: 0..999: <500 user, <750 client, <875 buyer, else customer
    use rand::Rng;
    let x = rand::thread_rng().gen_range(0..1000);
    match x {
        x if x < 500 => Endpoint::User,
        x if x < 750 => Endpoint::Client,
        x if x < 875 => Endpoint::Buyer,
        _ => Endpoint::Customer,
    }
}

fn build_query(i: i32) -> String {
    let si = i.to_string();
    format!(
        "?user[id]=123\
         &domain=advcache.example.com\
         &language=en\
         &picked[name]=helloworld\
         &picked[picked][name]=helloworld_foobarbazz\
         &picked[picked][picked][name]=helloworld_foobarbazz_null\
         &picked[picked][picked][picked][name]=helloworld_foobarbazz_null_{}\
         &picked[picked][picked][picked][picked][name]=helloworld_foobarbazz_null_{}_{}\
         &picked[picked][picked][picked][picked][picked][name]=helloworld_foobarbazz_null_{}_{}_{}\
         &picked[picked][picked][picked][picked][picked][picked]=null",
        si, si, si, si, si, si, si, si, si
    )
}

fn build_path_and_expected_prefix(ep: Endpoint, i: i32) -> (String, String) {
    let base = ep.path();
    let query = build_query(i);
    let path = format!("{}{}", base, query);
    (path, base.to_string())
}

fn parse_leading_int(s: &str) -> Option<i32> {
    let mut chars = s.chars().peekable();
    
    // Skip leading spaces
    while let Some(&ch) = chars.peek() {
        if ch == ' ' || ch == '\t' {
            chars.next();
        } else {
            break;
        }
    }
    
    // Parse digits
    let mut n = 0i32;
    let mut found_digit = false;
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            found_digit = true;
            let d = ch.to_digit(10).unwrap() as i32;
            n = n.checked_mul(10)?.checked_add(d)?;
            chars.next();
        } else {
            break;
        }
    }
    
    if found_digit {
        Some(n)
    } else {
        None
    }
}

fn desc_has_path_prefix(desc: &str, path: &str) -> bool {
    if desc.len() < path.len() || !desc.starts_with(path) {
        return false;
    }
    if desc.len() == path.len() {
        return true;
    }
    let next_char = desc.chars().nth(path.len());
    matches!(
        next_char,
        Some(' ' | '\t' | '\n' | '\r' | '.' | ',' | ';' | ':' | '-' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'')
    )
}

fn snippet(s: &str, n: usize) -> String {
    if n == 0 || s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    let base_url = format!("http://{}:{}", args.host, args.port);
    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout))
        .build()?;
    
    let deadline = Instant::now() + Duration::from_secs(args.duration);
    let use_mix = args.only.trim().is_empty();
    
    while Instant::now() < deadline {
        for i in args.start..=args.max {
            if Instant::now() >= deadline {
                return Ok(());
            }
            
            // Choose endpoint
            let ep = choose_endpoint(use_mix, &args.only);
            let (path, expected_path_prefix) = build_path_and_expected_prefix(ep, i);
            let url = format!("{}{}", base_url, path);
            
            // Build and send request
            let resp = match client
                .get(&url)
                .header("Accept-Encoding", "gzip, deflate, br")
                .header("Accept-Language", "en-US,en;q=0.9")
                .header("Content-Type", "application/json")
                .header("User-Agent", &args.ua)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    println!("❌ request i={} ep={}: {}", i, ep.name(), e);
                    continue;
                }
            };
            
            let status = resp.status();
            let ct = resp.headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("")
                .to_string();
            let ce = resp.headers()
                .get("content-encoding")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("")
                .to_string();
            
            // Read body (handles compression)
            let body = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    println!(
                        "❌ read i={} ep={}: {} (status={} ct={} enc={})",
                        i, ep.name(), e, status, ct, ce
                    );
                    continue;
                }
            };
            
            // Decompress if needed
            // Note: reqwest handles gzip/deflate automatically, but not brotli
            let body_bytes = match if ce == "br" {
                // Brotli decompression
                let mut decompressed = Vec::new();
                let mut decompressor = Decompressor::new(body.as_ref(), 4096);
                match decompressor.read_to_end(&mut decompressed) {
                    Ok(_) => Ok(decompressed),
                    Err(e) => Err(format!("brotli decompression error: {}", e)),
                }
            } else {
                Ok(body.to_vec())
            } {
                Ok(bytes) => bytes,
                Err(e) => {
                    println!(
                        "❌ decompress i={} ep={}: {} (status={} ct={} enc={})",
                        i, ep.name(), e, status, ct, ce
                    );
                    continue;
                }
            };
            
            // Parse JSON
            let json: Value = match serde_json::from_slice(&body_bytes) {
                Ok(j) => j,
                Err(e) => {
                    println!(
                        "❌ json i={} ep={}: {} (status={} ct={} enc={})",
                        i, ep.name(), e, status, ct, ce
                    );
                    println!("   body: {:?}", snippet(&String::from_utf8_lossy(&body_bytes), args.snip));
                    continue;
                }
            };
            
            // Extract title and description
            let title = json
                .get("data")
                .and_then(|d| d.get("attributes"))
                .and_then(|a| a.get("title"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            
            let desc = json
                .get("data")
                .and_then(|d| d.get("attributes"))
                .and_then(|a| a.get("description"))
                .and_then(|d| d.as_str())
                .unwrap_or("");
            
            // Validate title: must start with the same integer i we requested
            let t_num = match parse_leading_int(title) {
                Some(n) => n,
                None => {
                    println!(
                        "❌ title has no leading int i={} ep={} | title={:?}",
                        i, ep.name(), snippet(title, args.snip)
                    );
                    continue;
                }
            };
            
            if t_num != i {
                println!(
                    "❌ title int mismatch i={} ep={} | got={}, want={} | title={:?}",
                    i, ep.name(), t_num, i, snippet(title, args.snip)
                );
                continue;
            }
            
            // Validate description prefix: must start with exact path
            if !desc_has_path_prefix(desc, &expected_path_prefix) {
                println!("❌ desc path prefix mismatch i={} ep={}", i, ep.name());
                println!("   want prefix: {:?}", expected_path_prefix);
                println!("   got  desc  : {:?}", snippet(desc, args.snip));
                continue;
            }
            
            println!(
                "✅ ok i={} ep={} | title={} | desc^prefix={:?}…",
                i,
                ep.name(),
                t_num,
                snippet(&expected_path_prefix, 48)
            );
        }
    }
    
    Ok(())
}

