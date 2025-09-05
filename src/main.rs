#[cfg(feature = "cli")]
use clap::Parser;

#[cfg(feature = "cli")]
#[derive(Parser)]
#[command(name = "jsonschema2avro", about = "Convert JSON Schema to Avro Schema")]
struct Cli {
    /// Path or URL to the JSON Schema input
    #[arg(value_name = "JSONSCHEMA")]
    input: String,

    /// Path to the Avro schema output file
    #[arg(value_name = "AVRO")]
    output: String,

    /// Namespace override
    #[arg(long)]
    namespace: Option<String>,

    /// Utility namespace
    #[arg(long)]
    utility_namespace: Option<String>,

    /// Root record class name
    #[arg(long)]
    root_class_name: Option<String>,

    /// Split top-level records into separate files
    #[arg(long, default_value_t = false)]
    split_top_level_records: bool,
}

#[cfg(feature = "cli")]
fn main() {
    #[cfg(feature = "trace")]
    {
        use crustrace_mermaid::{GroupingMode, MermaidLayer};
        use tracing_subscriber::filter::LevelFilter;
        use tracing_subscriber::prelude::*;

        let mmd_layer = MermaidLayer::new()
            .with_mode(GroupingMode::MergeByName)
            .with_params_mode(crustrace_mermaid::ParamRenderMode::SingleNodeGrouped);

        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_span_events(
                        tracing_subscriber::fmt::format::FmtSpan::ENTER
                            | tracing_subscriber::fmt::format::FmtSpan::EXIT,
                    )
                    .with_filter(LevelFilter::INFO),
            )
            .with(mmd_layer)
            .init();
    }

    let cli = Cli::parse();

    if let Err(e) = jsonschema2avro::converter::convert_jsons_to_avro(
        &cli.input,
        &cli.output,
        cli.namespace.as_deref(),
        cli.utility_namespace.as_deref(),
        cli.root_class_name.as_deref(),
        cli.split_top_level_records,
    ) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("This binary is only available with the `cli` feature enabled.");
    std::process::exit(1);
}
