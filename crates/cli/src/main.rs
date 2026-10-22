mod pipeline_cmd;
mod preview;
mod spike;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "subtitler",
    version,
    about = "Broadcast-quality subtitle pipeline"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Compare STT providers on a real file
    Spike(spike::SpikeArgs),
    /// Transcribe a local file to SRT
    Transcribe(pipeline_cmd::TranscribeArgs),
    /// Transcribe and translate into target languages
    Run(pipeline_cmd::RunArgs),
    /// Process files and URLs through a resumable job queue
    Batch(pipeline_cmd::BatchArgs),
    /// List curated cheap LLM presets for OpenRouter
    LlmPresets,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Spike(args) => spike::run(args).await,
        Command::Transcribe(args) => pipeline_cmd::transcribe(args).await,
        Command::Run(args) => pipeline_cmd::run(args).await,
        Command::Batch(args) => pipeline_cmd::batch(args).await,
        Command::LlmPresets => {
            println!(
                "Curated OpenRouter LLM presets (multilingual, cheap):"
            );
            println!("pricing fluctuates; check openrouter.ai for current rates");
            for (name, slug) in subtitler_core::config::llm_presets::OPENROUTER {
                println!("  {name:22} → {slug}");
            }
            println!("\nuse with: subtitler run ... --llm-provider openrouter --llm-preset <name>");
            Ok(())
        }
    }
}
