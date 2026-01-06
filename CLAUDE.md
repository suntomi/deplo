# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Deplo is a Rust-based CLI tool that provides a unified CI/CD development experience across different CI/CD services (GitHub Actions, CircleCI). It solves CI/CD development pain points by enabling local job execution, remote debugging, and using TOML for configuration instead of YAML

## Key Commands

### Build and Test
- **Build**: `cargo build --release`
- **Test**: `cargo test` or `tools/scripts/test.sh`
- **Run a single test**: `cargo test test_name`
- **Build Linux binary**: `cargo run -- d product`

### Deplo-specific Commands
- **Initialize/update project CI configuration**: `deplo init` or `./deplow init`
- **Run integrate job locally**: `deplo i -r <release_target> <job_name>`
- **Run deploy job locally**: `deplo d -r <release_target> <job_name>`
- **Run job remotely**: `deplo i -r <release_target> <job_name> --remote`
- **Logged into job environment**: `deplo i -r <release_target> <job_name> sh`
- **Run CI workflow**: `deplo ci kick -r <release_target>`
- **Show all available subcommands**: `./deplow help` (and `help` works for showing instruction of every subcommands)

## Architecture and Structure

### Workspace Structure
- **cli/**: Command-line interface implementation
- **core/**: Core functionality library
  - `core/src/job/`: Job execution logic
  - `core/src/ci/`: CI service integrations (GitHub Actions, CircleCI)
  - `core/src/vcs/`: Version control integrations
  - `core/res/ci/`: CI configuration templates

### Configuration System
- **Deplo.toml**: Main configuration file defining jobs, workflows, and settings
- **.env**: Environment variables and secrets (not committed)
- **deplow**: Generated wrapper script ensuring version consistency

### Module System
- Modules stored in `tools/modules/` (slack, discord, prfilter, sample)
- Modules are reusable CI/CD components that work across different CI services
- Can be tested locally unlike traditional CI service-specific modules

### Job System
Jobs are filtered by changesets and support:
- Dependencies between jobs
- Dynamic outputs passed to dependent jobs
- Local and remote execution
- Auto-commit/PR after job completion

### Workflow Types
- **integrate**: Runs on pull requests to release targets
- **deploy**: Runs when release targets are updated
- **cron**: Scheduled workflows
- **repository**: Triggered by repository events
- **webapi_dispatch**: Manual or API-triggered workflows

### Release Targets
Defined in Deplo.toml:
- **nightly**: main branch
- **lab**: lab branch
- **prod**: tags starting with numbers (e.g., 0.1.10)
- **taglab**: tags starting with test- followed by numbers

## Development Workflow

1. Make changes in a development branch
2. Test jobs locally: `cargo run -- i -r <target> <job_name>`
3. Create PR to a release target branch
4. Deplo automatically runs integrate workflow based on changeset
5. After merge, deploy workflow runs on the release target

## Important Notes

- Always use `./deplow` if it exists to avoid version skew
- Secrets should be defined in .env, not in Deplo.toml
- Jobs only run when their changeset filters match modified files
- Use `--ref` option to test jobs against specific commits
- Remote job execution requires proper CI service credentials, which configured by .env