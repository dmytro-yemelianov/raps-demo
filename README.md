# RAPS Demo Workflows# RAPS Demo Workflows



[![CI](https://github.com/dmytro-yemelianov/raps-demo/actions/workflows/ci.yml/badge.svg)](https://github.com/dmytro-yemelianov/raps-demo/actions/workflows/ci.yml)Interactive demonstration system for the RAPS CLI showcasing Autodesk Platform Services (APS) workflows.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)## Overview



Interactive demonstration system for the [RAPS CLI](https://github.com/dmytro-yemelianov/raps) showcasing Autodesk Platform Services (APS) workflows.This application provides a Terminal User Interface (TUI) for discovering and executing demo workflows that demonstrate real-world APS usage patterns. Each workflow is a structured sequence of RAPS CLI commands with educational content, progress tracking, and resource management.



## Overview## Features



This application provides a Terminal User Interface (TUI) for discovering and executing demo workflows that demonstrate real-world APS usage patterns. Each workflow is a structured sequence of RAPS CLI commands with educational content, progress tracking, and resource management.- **Interactive TUI**: Browse and execute workflows through a terminal interface

- **Comprehensive Coverage**: Demos for all major APS services (OSS, Model Derivative, Data Management, Design Automation, ACC, Reality Capture, Webhooks)

## Features- **Educational Content**: Step-by-step explanations and links to APS documentation

- **Resource Management**: Automatic cleanup and cost awareness

- **Interactive TUI**: Browse and execute workflows through a terminal interface- **Progress Tracking**: Real-time execution monitoring with detailed logging

- **Comprehensive Coverage**: Demos for all major APS services (OSS, Model Derivative, Data Management, Design Automation, ACC, Reality Capture, Webhooks)

- **Educational Content**: Step-by-step explanations and links to APS documentation## Quick Start

- **Resource Management**: Automatic cleanup and cost awareness

- **Progress Tracking**: Real-time execution monitoring with detailed logging```bash

# Build the application

## Installationcargo build --release



### Prerequisites# Run the TUI interface

cargo run

- [Rust](https://rustup.rs/) 1.70.0 or later

- [RAPS CLI](https://github.com/dmytro-yemelianov/raps) installed and in PATH# Key Features in Demo

- Valid APS credentials (run `raps auth login` first)- **Dynamic Placeholders**: Use {uuid} and {timestamp} in YAML for unique resource naming.

- **Batch Processing**: Demo "Stapler Pipeline" uploads entire assembly folders.

### From Source- **Live Monitoring**: Real-time console logs and progress bars in TUI.



```bash## Demo Assets

# Clone the repository

git clone https://github.com/dmytro-yemelianov/raps-demo.gitThe demo system uses Inventor sample models located in `./Assets/Inventor/`. 

cd raps-demoThe primary demo assembly is the **Stapler** (located in `autodesk_inventor_2022_samples/Models/Assemblies/Stapler/Stapler.iam`).



# Build release binary## Project Structure

cargo build --release

```

# Run the TUIsrc/

./target/release/raps-demo├── main.rs           # Application entry point and CLI

```├── tui/              # Terminal User Interface components

├── demo/             # Demo manager and workflow discovery

## Usage├── workflow/         # Workflow execution engine

├── resource/         # Resource tracking and cleanup

### TUI Mode (Interactive)└── config/           # Configuration and authentication

```

```bash

# Launch the interactive terminal UI## Development

raps-demo

``````bash

# Run tests

**Keyboard shortcuts:**cargo test

- `↑/↓` or `j/k` - Navigate workflows

- `Enter` - Execute selected workflow# Run property-based tests

- `q` - Quitcargo test --features proptest



### CLI Mode (Non-interactive)# Format code

cargo fmt

```bash

# List available workflows# Run linter

raps-demo --no-tui --listcargo clippy



# Execute a specific workflow# Check for security vulnerabilities

raps-demo --no-tui --workflow oss-bucket-lifecyclecargo audit

``````



## Available Workflows## Requirements



| Workflow | Category | Description |- Rust 1.70.0 or later

|----------|----------|-------------|- RAPS CLI installed and configured

| `oss-bucket-lifecycle` | Object Storage | Create bucket, upload object, cleanup |- Valid APS credentials

| `stapler-md-pipeline` | Model Derivative | Upload Inventor assembly, SVF2 translation |

## License

## Creating Custom Workflows

MIT License - see LICENSE file for details.
Workflows are defined as YAML files in the `workflows/` directory:

```yaml
metadata:
  id: my-custom-workflow
  name: My Custom Workflow
  description: Demonstrates a custom APS workflow
  category: oss
  prerequisites:
    - type: authentication
      description: Valid APS credentials required
  estimated_duration: 60

steps:
  - id: create-bucket
    name: Create Bucket
    command:
      type: bucket
      action: create
      bucket_name: my-bucket-{uuid}

cleanup:
  - type: bucket
    action: delete
    bucket_name: my-bucket-{uuid}
```

## Project Structure

```
src/
├── main.rs           # Application entry point and CLI
├── tui/              # Terminal User Interface (ratatui)
├── demo/             # Demo manager and workflow discovery
├── workflow/         # Workflow execution engine
├── resource/         # Resource tracking and cleanup
├── config/           # Configuration and authentication
└── utils/            # Shared utilities
workflows/
├── oss/              # Object Storage demos
├── model-derivative/ # Model Derivative demos
└── ...               # Other service demos
```

## Development

```bash
# Run in development mode
cargo run

# Run tests
cargo test

# Format code
cargo fmt

# Run linter (allowing dead code for library APIs)
cargo clippy -- -A dead_code

# Build release
cargo build --release
```

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Related Projects

- [RAPS CLI](https://github.com/dmytro-yemelianov/raps) - The underlying CLI tool for APS operations
- [APS Documentation](https://aps.autodesk.com/developer/documentation) - Official Autodesk Platform Services docs
