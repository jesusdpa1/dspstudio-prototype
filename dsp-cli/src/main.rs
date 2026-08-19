mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;
use dsp_io::transmission::grpc_server::start_grpc_server;
use crate::cli::{Args, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    if let Some(command) = args.command {
        match command {
            Commands::CreateRecording {
                sampling_rate,
                recording_type,
                duration,
                channels,
                output,
                with_epochs,
            } => {
                commands::create_recording::run(
                    sampling_rate,
                    recording_type,
                    duration,
                    channels,
                    output,
                    with_epochs,
                )?;
            }
            Commands::Reencode { input, output, level } => {
                commands::reencode::run(input, output, level)?;
            }
            Commands::Tui { serve, path } => {
                commands::tui::run(serve, path).await?;
            }
        }
    } else if let Some(addr_str) = args.serve {
        let addr = addr_str.parse()?;
        println!("DSP-CLI: Starting gRPC server on {}", addr);
        if let Some(path) = args.path {
            println!("  Serving recording at: {:?}", path);
        }
        start_grpc_server(addr).await?;
    } else {
        println!("DSP-CLI: No action specified. Use 'create-recording', 'tui', or --serve <ADDR>.");
        if let Some(path) = args.path {
            println!("  Managing recording at: {:?}", path);
        }
    }

    Ok(())
}
