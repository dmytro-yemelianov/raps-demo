# Demo Workflows

This directory contains YAML workflow definitions that demonstrate various APS capabilities through the RAPS CLI.

## Structure

```
workflows/
├── oss/                    # Object Storage Service workflows
├── model-derivative/       # Model Derivative workflows  
├── data-management/        # Data Management workflows
├── design-automation/      # Design Automation workflows
├── acc/                    # Autodesk Construction Cloud workflows
├── reality-capture/        # Reality Capture workflows
├── webhooks/               # Webhook management workflows
└── end-to-end/            # Complete end-to-end workflows
```

## Workflow Format

Each workflow is defined in YAML format with the following structure:

```yaml
metadata:
  id: "workflow-id"
  name: "Human Readable Name"
  description: "Detailed description of what this workflow demonstrates"
  category: "WorkflowCategory"
  estimated_duration: "5m"
  cost_estimate:
    description: "Cost description"
    max_cost_usd: 0.10
  prerequisites:
    - type: "authentication"
      description: "Valid APS credentials"
  required_assets: ["path/to/asset.dwg"]

steps:
  - id: "step-1"
    name: "Step Name"
    description: "What this step does"
    command:
      type: "bucket"
      action: "create"
      params:
        bucket_name: "demo-bucket-{timestamp}"
    expected_duration: "30s"

cleanup:
  - command:
      type: "bucket"
      action: "delete"
      params:
        bucket_name: "{bucket_name}"
```

## Adding New Workflows

1. Create a new YAML file in the appropriate category directory
2. Follow the workflow format above
3. Test the workflow using the demo system
4. Update this README if adding a new category