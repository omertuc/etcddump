use std::sync::Arc;

use anyhow::{bail, ensure, Context, Result};
use clap::Parser;
use clio::*;
use etcd_client::{Client as EtcdClient, GetOptions};
use reqwest::Client;
use tokio::process::Command;

pub(crate) const OUGER_SERVER_PORT: u16 = 9998;

/// A program to regenerate cluster certificates, keys and tokens
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    /// etcd endpoint of etcd instance to recertify
    #[clap(long)]
    pub(crate) etcd_endpoint: String,

    /// Directory to recertify, such as /var/lib/kubelet, /etc/kubernetes and
    /// /etc/machine-config-daemon. Can specify multiple times
    #[clap(long, value_parser = clap::value_parser!(ClioPath).exists().is_dir())]
    pub(crate) output_dir: ClioPath,
}

pub(crate) struct ParsedCLI {
    pub(crate) etcd_endpoint: String,
    pub(crate) output_dir: ClioPath,
}

pub(crate) fn parse_cli() -> Result<ParsedCLI> {
    let cli = Cli::parse();

    Ok(ParsedCLI {
        etcd_endpoint: cli.etcd_endpoint,
        output_dir: cli.output_dir,
    })
}

pub(crate) fn set_max_open_files_limit() -> Result<()> {
    let mut current_limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    match unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut current_limit) } {
        0 => {}
        _ => {
            bail!("Failed to get current max open files limit");
        }
    }

    println!(
        "Current max open files soft {} hard {}",
        current_limit.rlim_cur, current_limit.rlim_max
    );

    let new_limit = libc::rlimit {
        rlim_cur: current_limit.rlim_max,
        rlim_max: current_limit.rlim_max,
    };

    println!(
        "Setting max open files soft {} hard {}",
        new_limit.rlim_cur, new_limit.rlim_max
    );

    match unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &new_limit) } {
        0 => {}
        _ => {
            bail!("Failed to set max open files limit");
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let parsed_cli = parse_cli().context("parsing CLI")?;
    set_max_open_files_limit().context("Setting open file limits to max")?;
    tokio::runtime::Runtime::new()?.block_on(async { main_internal(parsed_cli).await })
}

async fn ouger(ouger_path: &str, raw_etcd_value: &[u8]) -> Result<Vec<u8>> {
    let res = Client::new()
        .post(format!("http://localhost:{OUGER_SERVER_PORT}/{ouger_path}"))
        .body(raw_etcd_value.to_vec())
        .send()
        .await
        .context("ouger server not running")?;

    ensure!(
        res.status().is_success(),
        "ouger server returned non-success status code: {}",
        res.status()
    );
    ensure!(
        res.content_length().is_some(),
        "ouger server returned no content length"
    );

    Ok(res.bytes().await?.to_vec())
}

async fn main_internal(parsed_cli: ParsedCLI) -> Result<()> {
    let mut ouger_command = Command::new("ouger_server")
        .args(["--port", &OUGER_SERVER_PORT.to_string()])
        .spawn()?;

    let client = Arc::new(
        EtcdClient::connect([parsed_cli.etcd_endpoint.as_str()], None)
            .await
            .context("connecting to etcd")?,
    );

    let etcd_get_options = GetOptions::new()
        .with_prefix()
        .with_limit(0)
        .with_keys_only();

    let get_response = client
        .kv_client()
        .get("/", Some(etcd_get_options.clone()))
        .await?;

    let keys = get_response
        .kvs()
        .iter()
        .map(|k| Ok(k.key_str()?.to_string()))
        .collect::<Result<Vec<String>>>()?;

    let mut tasks = Vec::new();
    for key in keys {
        tasks.push(tokio::spawn(get_key(
            key,
            Arc::clone(&client),
            parsed_cli.output_dir.clone(),
        )));
    }

    for task in tasks {
        task.await??;
    }

    ouger_command.kill().await.context("killing ouger server")?;

    Ok(())
}

async fn get_key(
    key: String,
    client: Arc<EtcdClient>,
    output_dir: ClioPath,
) -> Result<(), anyhow::Error> {
    let get_result = client
        .kv_client()
        .get(key.clone(), None)
        .await
        .context("during etcd get")?;
    if let Some(value) = get_result.kvs().first() {
        let raw_etcd_value = value.value();

        let decoded_value = ouger("decode", raw_etcd_value)
            .await
            .context("decoding value with ouger")?;

        let output_file = output_dir.join(key.trim_start_matches('/'));

        std::fs::create_dir_all(output_file.parent().unwrap())?;
        std::fs::write(output_file, decoded_value)?;
    };
    Ok(())
}
